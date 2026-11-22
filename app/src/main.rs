#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod audio;
mod discord;
mod discovery;
mod ffi;
mod keys;
mod pad;
mod pages;
mod psn;
mod settings;
mod splash;
mod store;
mod winui;
mod lang;
mod lang_en;
mod theme;
mod tray;
mod update;
mod video;

use discovery::{Console, State};
use eframe::egui::{
    self, epaint::RectShape, Align, Align2, Color32, FontId, Key, Layout, Pos2, Rect, RichText, Rounding, Shape, Stroke,
    TextureHandle, TextureOptions, Vec2,
};
use ffi::{Paired, Stream, StreamParams};
use settings::Settings;
use theme::*;
use egui_phosphor::regular as ic;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

const GREEN: Color32 = Color32::from_rgb(52, 211, 153);
const YELLOW: Color32 = Color32::from_rgb(251, 191, 36);
const RED: Color32 = Color32::from_rgb(248, 113, 113);
const TEXT_DIM: Color32 = Color32::from_rgb(128, 141, 170);
const VERSION: &str = "0.34";

#[derive(PartialEq, Clone, Copy)]
enum Page {
    Home,
    News,
    Consoles,
    Settings,
    Help,
}

/// Adresse des Meldeservers (Cloudflare Worker). Leer = Meldungen laufen über GitHub.
const RELAY_URL: &str = "https://overlime.info/meldung.php";

/// Ergebnis des Netzwerk-Tests (Latenz zur Konsole).
#[derive(Clone)]
pub struct NetResult {
    pub avg: f32,
    pub jitter: f32,
    pub fails: u32,
    pub n: u32,
}

/// Empfehlung aus dem Netzwerk-Test: (Auflösung, FPS-Index, Mbit/s, Bewertung).
pub fn net_recommend(r: &NetResult) -> (usize, usize, f32, &'static str) {
    if r.fails > 0 && r.fails * 4 > r.n {
        (1, 0, 8.0, "Unruhig")
    } else if r.avg <= 10.0 && r.jitter <= 4.0 && r.fails == 0 {
        (2, 1, 50.0, "Sehr gut")
    } else if r.avg <= 25.0 && r.jitter <= 10.0 {
        (2, 1, 35.0, "Gut")
    } else if r.avg <= 50.0 && r.jitter <= 20.0 {
        (1, 1, 20.0, "Mittel")
    } else {
        (1, 0, 10.0, "Schwach")
    }
}

enum Msg {
    Found(Vec<Console>),
    Error(String),
    Info(String),
    PsnOk([u8; 8], String),
    PsnErr(String),
    PairOk(Paired),
    PairErr(String),
    StreamStarted(Stream, String),
    StreamErr(String),
    UpdateAvail(update::Release),
    UpdateNone,
    DiagPort(bool, String),
    NetTest(NetResult),
    ReportDone(bool, String),
    AudioDevices(Vec<String>),
    News(Vec<update::NewsItem>),
    NewsErr(String),
    UpdateCheckFailed(String),
    UpdateErr(String),
}

#[derive(PartialEq, Clone, Copy)]
enum Step {
    Psn,
    Pin,
    Busy,
    Done,
    Failed,
}

struct PairDialog {
    console: Console,
    step: Step,
    pasted: String,
    pin: String,
    message: String,
}

struct Session {
    stream: Arc<Stream>,
    audio: Option<audio::Audio>,
    audio_err: Option<String>,
    tex: Option<TextureHandle>,
    buf: Vec<u8>,
    gpu: Arc<video::VideoGpu>,
    use_gpu: bool,
    has_video: bool,
    vsize: (usize, usize),
    nv12: Vec<u8>,
    ping: f32,
    dec_ms: f32,
    kbps: u32,
    loss_prev: i32,
    loss_rate: f32,
    hist: Vec<(f32, f32, f32)>,
    hist_t: Instant,
    loss_t: Instant,
    name: String,
    started: Instant,
    fps_count: u32,
    fps_t: Instant,
    fps: u32,
    frames_total: u64,
    state: i32,
    ended_error: bool,
    message: String,
    size: (i32, i32),
    lost: i32,
    last_input: Instant,
    started_unix: i64,
}

struct App {
    page: Page,
    page_since: f64,
    consoles: Vec<Console>,
    first_seen: HashMap<String, f64>,
    misses: u8,
    local_ip: String,
    settings: Settings,
    saved: Settings,
    paired: Vec<Paired>,
    dialog: Option<PairDialog>,
    session: Option<Session>,
    starting: Option<String>,
    update: Option<update::Release>,
    update_busy: bool,
    upd_progress: Arc<std::sync::Mutex<update::Progress>>,
    upd_shown: f32,
    welcome: Option<(String, String)>,
    welcome_start: Option<f64>,
    update_checked: bool,
    update_msg: String,
    update_err: String,
    skip_login: bool,
    login_pasted: String,
    login_step: usize,
    set_cat: usize,
    help_tab: usize,
    wiz: Option<pad::Wizard>,
    news_lang: usize,
    post_update: Option<String>,
    post_t0: Option<f64>,
    report_open: bool,
    crash_pending: bool,
    tour: Option<usize>,
    tour_checked: bool,
    net_res: Option<NetResult>,
    profile_active: Option<String>,
    saved_global: Option<(usize, usize, f32, usize)>,
    net_hint: Option<f32>,
    report_text: String,
    report_state: u8,
    report_msg: String,
    report_last: f64,
    set_q: String,
    clip_seen: f64,
    login_busy: bool,
    login_error: String,
    start_error: Option<String>,
    log: String,
    pad: pad::Pad,
    nav_prev: u32,
    stream_menu: bool,
    show_keys: bool,
    show_hud: bool,
    capturing: Option<usize>,
    capture_since: f64,
    key_note: String,
    allow_close: bool,
    themes: Vec<theme::Preset>,
    titlebar_dirty: bool,
    title_tries: u8,
    discord: discord::Handle,
    app_start_unix: i64,
    icon_tex: Option<TextureHandle>,
    prev_ready: HashMap<String, bool>,
    auto_done: bool,
    connect_now: bool,
    tray_auto: bool,
    auto_wake_done: bool,
    diag_port: Option<(bool, String)>,
    diag_busy: bool,
    applied_theme: String,
    toasts: Vec<(String, u8, f64)>,
    now: f64,
    pad_name_cache: String,
    shot_requested: bool,
    rec_toggle: bool,
    rec_started: Option<Instant>,
    rec_name: String,
    feedback: Option<bool>,
    audio_devices: Vec<String>,
    audio_test: Option<(audio::Audio, f64)>,
    muted: bool,
    gl_ok: bool,
    gl_info: String,
    quick: bool,
    last_connect: Option<(Console, Paired, (usize, usize, i32, usize))>,
    pending_reconnect: Option<(Console, Paired, Instant)>,
    reconnecting: bool,
    reconnect_msg: String,
    quality_retry: u32,
    reconnect_delay_ms: u64,
    auto_retries: u32,
    update_click: bool,
    news: Vec<update::NewsItem>,
    news_loading: bool,
    news_shown: usize,
    news_err: String,
    news_fetched: Option<Instant>,
    app_start: Instant,
    check_click: bool,
    update_checking: bool,
    update_checked_at: Option<Instant>,
    last_update_check: Instant,
    demo_gpu: Option<Arc<video::VideoGpu>>,
    demo_base: Vec<u8>,
    demo_step: usize,
    close_hint: bool,
    fullscreen: bool,
    status: String,
    searching: bool,
    last_scan: Instant,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

fn log_path() -> Option<std::path::PathBuf> {
    Some(std::path::PathBuf::from(std::env::var("APPDATA").ok()?).join("overlime").join("log.txt"))
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_fonts(&cc.egui_ctx);
        theme::ensure_example();
        let themes = theme::all();
        let st0 = Settings::load();
        if let Some(p) = themes.iter().find(|p| p.name == st0.theme) {
            theme::set(p.theme);
        }
        setup_style(&cc.egui_ctx);
        let (tx, rx) = channel();
        {
            // Tonausgänge im Hintergrund suchen (dauert ca. 0,3 s)
            let txa = tx.clone();
            std::thread::spawn(move || {
                let _ = txa.send(Msg::AudioDevices(audio::list_devices()));
            });
        }
        let settings0 = Settings::load();
        let prev_version = settings0.last_version.clone();
        let show_welcome = settings0.welcome_anim;
        let mut settings = settings0.clone();
        settings.last_version = VERSION.to_string();
        let mut app = Self {
            page: Page::Home,
            page_since: 0.0,
            consoles: vec![],
            first_seen: HashMap::new(),
            misses: 0,
            local_ip: discovery::local_ip().map(|i| i.to_string()).unwrap_or_else(|| "?".into()),
            saved: settings0,
            settings,
            paired: store::load(),
            dialog: None,
            session: None,
            starting: None,
            update: None,
            update_busy: false,
            upd_progress: Arc::new(std::sync::Mutex::new(update::Progress::default())),
            upd_shown: 0.0,
            welcome: if show_welcome && !prev_version.is_empty() && prev_version != VERSION { Some((prev_version.clone(), VERSION.to_string())) } else { None },
            welcome_start: None,
            update_checked: false,
            update_msg: String::new(),
            update_err: String::new(),
            skip_login: false,
            login_pasted: String::new(),
            set_cat: std::env::args().find_map(|a| a.strip_prefix("--set-cat=").and_then(|v| v.parse().ok())).unwrap_or(0),
            help_tab: 0,
            wiz: None,
            news_lang: usize::MAX,
            post_update: std::env::args().find_map(|a| a.strip_prefix("--from=").map(|v| v.to_string())).filter(|_| std::env::args().any(|a| a == "--after-update")),
            post_t0: None,
            net_res: None,
            profile_active: None,
            saved_global: None,
            net_hint: None,
            crash_pending: false,
            tour: None,
            tour_checked: false,
            report_open: std::env::args().any(|a| a == "--report-dialog"),
            report_text: String::new(),
            report_state: 0,
            report_msg: String::new(),
            report_last: 0.0,
            set_q: std::env::args().find_map(|a| a.strip_prefix("--set-q=").map(|v| v.to_string())).unwrap_or_default(),
            login_step: std::env::args().find_map(|a| a.strip_prefix("--login-step=").and_then(|v| v.parse().ok())).unwrap_or(0),
            clip_seen: 0.0,
            login_busy: false,
            login_error: String::new(),
            start_error: None,
            log: String::new(),
            pad: pad::Pad::new(),
            nav_prev: 0,
            stream_menu: false,
            show_keys: false,
            show_hud: true,
            capturing: None,
            capture_since: 0.0,
            key_note: String::new(),
            allow_close: false,
            themes,
            titlebar_dirty: true,
            title_tries: 0,
            discord: discord::spawn(),
            app_start_unix: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0),
            icon_tex: None,
            prev_ready: HashMap::new(),
            auto_done: false,
            connect_now: std::env::args().any(|a| a == "--connect-now"),
            tray_auto: tray::autostart_enabled(),
            auto_wake_done: false,
            diag_port: None,
            diag_busy: false,
            applied_theme: String::new(),
            toasts: Vec::new(),
            now: 0.0,
            pad_name_cache: String::new(),
            shot_requested: false,
            rec_toggle: false,
            rec_started: None,
            rec_name: String::new(),
            feedback: None,
            audio_devices: Vec::new(),
            audio_test: None,
            muted: false,
            gl_ok: false,
            gl_info: String::new(),
            quick: false,
            last_connect: None,
            pending_reconnect: None,
            reconnecting: false,
            reconnect_msg: String::new(),
            quality_retry: 0,
            reconnect_delay_ms: 2500,
            auto_retries: 0,
            update_click: false,
            news: Vec::new(),
            news_loading: false,
            news_shown: 1,
            news_err: String::new(),
            news_fetched: None,
            app_start: Instant::now(),
            check_click: false,
            update_checking: false,
            update_checked_at: None,
            last_update_check: Instant::now(),
            demo_gpu: None,
            demo_base: Vec::new(),
            demo_step: usize::MAX,
            close_hint: false,
            fullscreen: false,
            status: String::new(),
            searching: false,
            last_scan: Instant::now(),
            tx,
            rx,
        };
        if std::env::args().any(|a| a == "--demo") {
            app.local_ip = "192.168.0.10".into();
            if std::env::args().any(|a| a == "--demo-welcome") {
                app.welcome = Some(("0.10".into(), "0.11".into()));
            }
            if std::env::args().any(|a| a == "--demo-update-avail") {
                app.update = Some(update::Release {
                    version: "0.20".into(),
                    url: String::new(),
                    sha_url: String::new(),
                    sig_url: String::new(),
                    notes: "### Neu in 0.20\n- **Neue Funktion** zum Ausprobieren\n- Schnelleres Verbinden\n- Viele kleine Verbesserungen\n".into(),
                });
                app.update_checked = true;
            }
            if std::env::args().any(|a| a == "--demo-update") {
                app.update = Some(update::Release { version: "0.11".into(), url: String::new(), sha_url: String::new(), sig_url: String::new(), notes: String::new() });
                app.update_busy = true;
                let prog = app.upd_progress.clone();
                std::thread::spawn(move || {
                    let t0 = Instant::now();
                    loop {
                        let el = t0.elapsed().as_secs_f32();
                        let (stage, pct) = if el < 3.0 { (0, 0.0) } else if el < 8.0 { (1, (el - 3.0) / 5.0 * 100.0) } else if el < 12.2 { (2, 0.0) } else if el < 16.8 { (3, 0.0) } else { (4, 0.0) };
                        if let Ok(mut g) = prog.lock() {
                            g.stage = stage;
                            g.pct = pct;
                            g.hash = "9716ead1f3a7e319052df3798585d72bfa59fcf966198de96dc70fefe3390899".to_string();
                            g.total = 35_000_000;
                            g.bytes = if stage >= 2 { 35_000_000 } else { (pct / 100.0 * 35_000_000.0) as u64 };
                            g.speed = if stage == 1 { 7_300_000.0 } else { 0.0 };
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                });
            }
            let mk = |name: &str, ps5: bool, state: State, ip: &str, a: &str| Console {
                addr: ip.into(),
                name: name.into(),
                id: ip.into(),
                is_ps5: ps5,
                state,
                system_version: "09000000".into(),
                running_app: a.into(),
            };
            app.consoles = vec![
                mk("PS5-Wohnzimmer", true, State::Ready, "192.168.178.40", "EA SPORTS FC 26"),
                mk("PS4-Zimmer", false, State::Standby, "192.168.178.41", ""),
            ];
            app.last_scan = Instant::now() + Duration::from_secs(100_000);
            if std::env::args().any(|a| a == "--settings") {
                app.page = Page::Settings;
            }
            if std::env::args().any(|a| a == "--demo-feedback") {
                app.feedback = Some(false);
            }
            if std::env::args().any(|a| a == "--demo-feedback-error") {
                app.feedback = Some(true);
            }
            app.load_news();
            if std::env::args().any(|a| a == "--news-page") {
                app.page = Page::News;
            }
            if std::env::args().any(|a| a == "--diag-page") {
                app.page = Page::Help;
                app.help_tab = std::env::args().find_map(|a| a.strip_prefix("--help-tab=").and_then(|v| v.parse().ok())).unwrap_or(0);
            }
            if std::env::args().any(|a| a == "--help-page") {
                app.page = Page::Help;
            }
            if std::env::args().any(|a| a == "--consoles") {
                app.page = Page::Consoles;
            }
            if std::env::args().any(|a| a == "--dialog") {
                app.dialog = Some(PairDialog {
                    console: app.consoles[0].clone(),
                    step: Step::Psn,
                    pasted: String::new(),
                    pin: String::new(),
                    message: String::new(),
                });
            }
            if std::env::args().any(|a| a == "--dialog-pin") {
                app.dialog = Some(PairDialog {
                    console: app.consoles[0].clone(),
                    step: Step::Pin,
                    pasted: String::new(),
                    pin: "48271953".into(),
                    message: String::new(),
                });
            }
            return app;
        }
        update::cleanup_old();
        if !RELAY_URL.is_empty() && crash_path().map(|p| p.exists()).unwrap_or(false) && !std::env::args().any(|a| a == "--no-crash-prompt") {
            app.report_open = true;
            app.crash_pending = true;
            app.report_text = "OVERLIME ist beim letzten Mal unerwartet beendet worden.".to_string();
        }
        app.load_news();
        if update::enabled() {
            let tx = app.tx.clone();
            std::thread::spawn(move || {
                match update::check(VERSION) {
                    Ok(Some(r)) => {
                        let _ = tx.send(Msg::UpdateAvail(r));
                    }
                    Ok(None) => {
                        let _ = tx.send(Msg::UpdateNone);
                    }
                    Err(_) => {}
                }
            });
        }
        app.search();
        app
    }

    fn search(&mut self) {
        self.searching = true;
        self.last_scan = Instant::now();
        let tx = self.tx.clone();
        let manual = self.settings.manual_ip.trim().to_string();
        std::thread::spawn(move || {
            let mut all = match discovery::discover(None, Duration::from_millis(2500)) {
                Ok(l) => l,
                Err(e) => {
                    let _ = tx.send(Msg::Error(e.to_string()));
                    return;
                }
            };
            if !manual.is_empty() {
                if let Ok(extra) = discovery::discover(Some(&manual), Duration::from_millis(2500)) {
                    for c in extra {
                        if !all.iter().any(|x| x.addr == c.addr) {
                            all.push(c);
                        }
                    }
                }
            }
            let _ = tx.send(Msg::Found(all));
        });
    }

    fn paired_for(&self, c: &Console) -> Option<Paired> {
        self.paired.iter().find(|p| p.console_id == c.id).cloned()
    }

    fn wake(&mut self, c: &Console, p: &Paired) {
        let Some(cred) = p.wake_credential() else {
            self.status = "Aufwecken nicht möglich – bitte Konsole neu koppeln.".into();
            return;
        };
        let tx = self.tx.clone();
        let (addr, ps5) = (c.addr.clone(), c.is_ps5);
        std::thread::spawn(move || {
            let m = match discovery::wakeup(&addr, cred, ps5) {
                Ok(()) => Msg::Info("Aufwecksignal gesendet – in ein paar Sekunden ist die Konsole bereit.".into()),
                Err(e) => Msg::Error(e.to_string()),
            };
            let _ = tx.send(m);
        });
    }

    fn open_dialog(&mut self, c: &Console) {
        let step = if psn::id_from_b64(&self.settings.psn_account).is_some() { Step::Pin } else { Step::Psn };
        self.dialog = Some(PairDialog { console: c.clone(), step, pasted: String::new(), pin: String::new(), message: String::new() });
    }

    fn connect(&mut self, c: &Console, p: &Paired) {
        if self.starting.is_some() || self.session.is_some() {
            return;
        }
        self.start_error = None;
        self.profile_apply(&c.id);
        self.starting = Some(c.name.clone());
        self.last_connect = Some((c.clone(), p.clone(), (self.settings.resolution, self.settings.fps, self.settings.bitrate as i32, self.settings.codec)));
        let (w, h) = [(960, 540), (1280, 720), (1920, 1080)][self.settings.resolution];
        let fps = [30, 60][self.settings.fps];
        let hevc = match self.settings.codec {
            0 => p.ps5,
            1 => false,
            _ => true,
        };
        let params = StreamParams {
            host: c.addr.clone(),
            paired: p.clone(),
            width: w,
            height: h,
            fps,
            bitrate_kbps: (self.settings.bitrate * 1000.0) as i32,
            hevc,
            hw_decode: true,
            dualsense: self.settings.ds_features && self.pad.is_dualsense(),
        };
        let tx = self.tx.clone();
        let name = c.name.clone();
        std::thread::spawn(move || {
            let m = match Stream::start(&params) {
                Ok(s) => Msg::StreamStarted(s, name),
                Err(e) => Msg::StreamErr(e),
            };
            let _ = tx.send(m);
        });
    }

    /// Wendet die eigene Qualität der Konsole an (falls eingerichtet) und merkt sich die allgemeinen Werte.
    fn profile_apply(&mut self, console_id: &str) {
        self.profile_release();
        let Some(pr) = self.settings.profiles.iter().find(|(id, _)| id == console_id).map(|(_, p)| p.clone()) else { return };
        self.saved_global = Some((self.settings.resolution, self.settings.fps, self.settings.bitrate, self.settings.codec));
        self.profile_active = Some(console_id.to_string());
        self.settings.resolution = pr.resolution;
        self.settings.fps = pr.fps;
        self.settings.bitrate = pr.bitrate;
        self.settings.codec = pr.codec;
    }

    /// Speichert Änderungen im Profil und stellt die allgemeinen Werte wieder her.
    fn profile_release(&mut self) {
        if let Some(id) = self.profile_active.take() {
            let cur = settings::Profile { resolution: self.settings.resolution, fps: self.settings.fps, bitrate: self.settings.bitrate, codec: self.settings.codec };
            if let Some(e) = self.settings.profiles.iter_mut().find(|(i, _)| *i == id) {
                e.1 = cur;
            }
            if let Some((r, f, b, c)) = self.saved_global.take() {
                self.settings.resolution = r;
                self.settings.fps = f;
                self.settings.bitrate = b;
                self.settings.codec = c;
            }
        }
    }

    fn stop_stream(&mut self, ctx: &egui::Context) {
        let info = self.session.as_ref().map(|s| (s.state, s.ended_error, s.started.elapsed().as_secs()));
        if let Some(s) = self.session.as_ref() {
            let secs = s.started.elapsed().as_secs();
            if s.state == 1 && secs >= 30 && s.lost >= 40 {
                let rec = (self.settings.bitrate * 0.7).round().max(5.0);
                if rec < self.settings.bitrate {
                    self.net_hint = Some(rec);
                    self.toasts.push((format!("Viele Pakete verloren. Tipp: Bitrate auf {rec} Mbit/s senken (Hilfe & Diagnose > Prüfung)."), 1, self.now));
                }
            }
        }
        self.stream_menu = false;
        self.show_keys = false;
        self.close_hint = false;
        if self.rec_started.take().is_some() {
            let name = std::mem::take(&mut self.rec_name);
            if let Some(s) = self.session.as_ref() {
                s.stream.record_stop();
            }
            self.toasts.push((format!("Aufnahme gespeichert: {name}"), 2, self.now));
        }
        self.session = None; // Drop beendet die Sitzung
        self.profile_release();
        if self.settings.ask_feedback {
            if let Some((state, err, secs)) = info {
                if err {
                    self.feedback = Some(true);
                } else if state == 1 && secs >= 15 {
                    self.feedback = Some(false);
                }
            }
        }
        self.starting = None;
        if self.fullscreen {
            self.fullscreen = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
    }

    fn pump_log(&mut self) {
        let l = mask_ips(&ffi::take_log());
        if !l.is_empty() {
            self.log.push_str(&l);
            if self.log.len() > 60_000 {
                let cut = self.log.len() - 40_000;
                self.log = self.log[cut..].to_string();
            }
            if let Some(p) = log_path() {
                if let Some(d) = p.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                    let _ = f.write_all(l.as_bytes());
                }
            }
        }
    }

    fn handle_messages(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Found(l) => {
                    self.searching = false;
                    if l.is_empty() {
                        self.misses = self.misses.saturating_add(1);
                        if self.misses >= 2 {
                            self.consoles.clear();
                        }
                    } else {
                        self.misses = 0;
                        let mut l = l;
                        l.sort_by(|a, b| a.name.cmp(&b.name));
                        self.consoles = l;
                        self.after_scan();
                    }
                }
                Msg::Error(e) => {
                    self.toasts.push((format!("Fehler: {}", mask_ips(&e)), 1, self.now));
                    self.status = format!("Fehler: {}", mask_ips(&e));
                    self.searching = false;
                }
                Msg::Info(s) => {
                    self.toasts.push((s.clone(), 0, self.now));
                    self.status = s;
                }
                Msg::PsnOk(id, name) => {
                    self.toasts.push(("Bei PlayStation angemeldet".to_string(), 2, self.now));
                    self.settings.psn_account = psn::id_to_b64(id);
                    self.settings.psn_name = name;
                    self.login_busy = false;
                    self.login_error.clear();
                    if let Some(d) = self.dialog.as_mut() {
                        d.step = Step::Pin;
                        d.message.clear();
                    }
                }
                Msg::PsnErr(e) => {
                    self.login_busy = false;
                    self.login_error = e.clone();
                    if let Some(d) = self.dialog.as_mut() {
                        d.step = Step::Psn;
                        d.message = e;
                    }
                }
                Msg::PairOk(p) => {
                    self.toasts.push((format!("{} gekoppelt", p.nickname), 2, self.now));
                    self.paired.retain(|x| x.console_id != p.console_id);
                    self.paired.push(p);
                    store::save(&self.paired);
                    if let Some(d) = self.dialog.as_mut() {
                        d.step = Step::Done;
                    }
                }
                Msg::PairErr(e) => {
                    if let Some(d) = self.dialog.as_mut() {
                        d.step = Step::Failed;
                        d.message = mask_ips(&e);
                    }
                }
                Msg::StreamStarted(stream, name) => {
                    self.reconnecting = false;
                    self.starting = None;
                    if let Some(u) = self.update.as_ref() {
                        self.toasts.push((format!("Update {} wartet – nach dem Spiel installieren", u.version), 0, self.now));
                    }
                    if !self.gl_ok {
                        stream.set_rgba(true);
                    }
                    let stream = Arc::new(stream);
                    let (audio, audio_err) = match audio::start(stream.clone(), &self.settings.audio_device, self.settings.volume) {
                        Ok(a) => {
                            let info = a.shared.info.lock().map(|g| g.clone()).unwrap_or_default();
                            self.log.push_str(&format!("[I] Audio: {info}\n"));
                            (Some(a), None)
                        }
                        Err(e) => {
                            self.log.push_str(&format!("[E] Audio: {e}\n"));
                            (None, Some(e))
                        }
                    };
                    self.session = Some(Session {
                        stream,
                        audio,
                        audio_err,
                        tex: None,
                        buf: Vec::new(),
                        gpu: video::VideoGpu::new(),
                        use_gpu: self.gl_ok,
                        has_video: false,
                        vsize: (0, 0),
                        nv12: Vec::new(),
                        ping: -1.0,
                        dec_ms: 0.0,
                        kbps: 0,
                        loss_prev: 0,
                        loss_rate: 0.0,
                        hist: Vec::new(),
                        hist_t: Instant::now(),
                        loss_t: Instant::now(),
                        name,
                        started: Instant::now(),
                        fps_count: 0,
                        fps_t: Instant::now(),
                        fps: 0,
                        frames_total: 0,
                        state: 0,
                        ended_error: false,
                        message: String::new(),
                        size: (0, 0),
                        lost: 0,
                        last_input: Instant::now(),
                        started_unix: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0),
                    });
                    if self.settings.fullscreen {
                        self.fullscreen = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                    }
                }
                Msg::StreamErr(e) => {
                    self.reconnecting = false;
                    self.starting = None;
                    if self.quality_retry > 0 {
                        if let Some((c, p, _)) = self.last_connect.clone() {
                            self.quality_retry -= 1;
                            self.reconnecting = true;
                            self.reconnect_delay_ms = 4000;
                            self.reconnect_msg = "Die Konsole braucht einen Moment, neuer Versuch …".to_string();
                            self.pending_reconnect = Some((c, p, Instant::now()));
                            continue;
                        }
                    }
                    self.start_error = Some(mask_ips(&e));
                }
                Msg::UpdateAvail(r) => {
                    self.update_checking = false;
                    self.update_checked_at = Some(Instant::now());
                    if self.update.as_ref().map(|u| u.version != r.version).unwrap_or(true) {
                        let during_game = self.session.is_some();
                        let text = if during_game {
                            format!("Update {} verfügbar – nach dem Spiel installieren", r.version)
                        } else {
                            format!("Update {} verfügbar", r.version)
                        };
                        self.toasts.push((text, 0, self.now));
                    }
                    // Nie mitten im Verbinden installieren (z. B. Klick auf das Symbol neben der Uhr): das würde die Verbindung abbrechen
                    let connecting = self.starting.is_some() || self.reconnecting || self.pending_reconnect.is_some() || self.connect_now || self.settings.autoconnect || self.dialog.is_some() || self.report_open;
                    if self.settings.auto_update && !self.update_busy && self.session.is_none() && !connecting && self.app_start.elapsed() < Duration::from_secs(90) {
                        self.update_busy = true;
                        let tx = self.tx.clone();
                        let rr = r.clone();
                        *self.upd_progress.lock().unwrap() = update::Progress::default();
                        self.upd_shown = 0.0;
                        let prog = self.upd_progress.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = update::install_ui(&rr, prog) {
                                let _ = tx.send(Msg::UpdateErr(e));
                            }
                        });
                    }
                    self.update = Some(r);
                }
                Msg::UpdateErr(e) => {
                    // update_busy bleibt an, damit der Fehler im Update-Bildschirm sichtbar ist
                    self.update_err = e;
                }
                Msg::News(v) => {
                    self.news = v;
                    self.news_loading = false;
                    self.news_err.clear();
                    self.news_fetched = Some(Instant::now());
                }
                Msg::NewsErr(e) => {
                    self.news_loading = false;
                    self.news_err = mask_ips(&e);
                    self.news_fetched = Some(Instant::now());
                }
                Msg::ReportDone(ok, text) => {
                    if ok && self.crash_pending {
                        if let Some(p) = crash_path() {
                            let _ = std::fs::remove_file(p);
                        }
                        self.crash_pending = false;
                    }
                    self.report_state = if ok { 2 } else { 3 };
                    self.report_msg = text;
                }
                Msg::AudioDevices(v) => {
                    self.audio_devices = v;
                }
                Msg::NetTest(r) => {
                    self.net_res = Some(r);
                }
                Msg::DiagPort(ok, text) => {
                    self.diag_busy = false;
                    self.diag_port = Some((ok, text));
                }
                Msg::UpdateCheckFailed(e) => {
                    self.update_checking = false;
                    self.update_msg = format!("Prüfung fehlgeschlagen: {}", mask_ips(&e));
                }
                Msg::UpdateNone => {
                    self.update_checking = false;
                    self.update_checked_at = Some(Instant::now());
                    self.update_checked = true;
                    self.update_msg = "Du hast die neueste Version.".into();
                }
            }
        }
    }
}

/// Übersetzt chiaki-Fehlertexte in verständliches Deutsch.
/// Ersetzt IPv4-Adressen (auch mit :Port) durch x.x.x.x, damit nirgends IPs auftauchen.
fn mask_ips(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let mut j = i;
            let mut groups = 0;
            let mut ok = true;
            loop {
                let st = j;
                while j < b.len() && b[j].is_ascii_digit() && j - st < 3 {
                    j += 1;
                }
                if j == st {
                    ok = false;
                    break;
                }
                groups += 1;
                if groups == 4 {
                    break;
                }
                if j < b.len() && b[j] == b'.' {
                    j += 1;
                } else {
                    ok = false;
                    break;
                }
            }
            if ok && groups == 4 {
                out.push_str("x.x.x.x");
                if j < b.len() && b[j] == b':' {
                    let mut k = j + 1;
                    while k < b.len() && b[k].is_ascii_digit() {
                        k += 1;
                    }
                    j = k;
                }
                i = j;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// "14230000" -> "14.23"
fn pretty_version(v: &str) -> String {
    match v.trim().parse::<u32>() {
        Ok(n) if n > 0 => format!("{}.{:02}", n / 1_000_000, (n / 10_000) % 100),
        _ => "–".to_string(),
    }
}

/// Texte für den Discord-Status: nur Spiel und Konsole. mode: 0 = Übersicht, 1 = verbindet, 2 = spielt.
fn presence_lines(st: &Settings, mode: u8, game: &str, ps5: bool, found: usize) -> (String, String) {
    let console = if ps5 { "PlayStation 5" } else { "PlayStation 4" };
    match mode {
        2 => {
            if st.discord_game && !game.is_empty() {
                (lang::tr(&format!("Spielt {game}")), lang::tr(&format!("auf {console}")))
            } else {
                (lang::tr(&format!("Spielt auf {console}")), "OVERLIME Remote Play".to_string())
            }
        }
        1 => ("Verbindet mit der Konsole".to_string(), "OVERLIME Remote Play".to_string()),
        _ => (if found > 0 { "Bereit zum Zocken".to_string() } else { "Sucht eine Konsole".to_string() }, "OVERLIME Remote Play".to_string()),
    }
}

fn german_reason(msg: &str) -> String {
    let m = msg.to_lowercase();
    let t = if m.contains("unknown session request") || m.contains("ctrl failed to connect") {
        "Die Konsole antwortet nicht. Ist sie eingeschaltet (oder im Ruhemodus mit Netzwerk-Aufwecken) und im selben Netzwerk?"
    } else if m.contains("connection refused") {
        "Die Konsole hat die Verbindung abgelehnt. Ist Remote Play in den Konsoleneinstellungen aktiviert und die Konsole gekoppelt?"
    } else if m.contains("already in use") {
        "Remote Play wird auf der Konsole schon benutzt (anderes Gerät oder Sony-App). Dort zuerst beenden."
    } else if m.contains("crashed") {
        "Remote Play auf der Konsole ist abgestürzt. Konsole kurz neu starten."
    } else if m.contains("version mismatch") {
        "Die Konsolen-Software passt nicht zu dieser App-Version."
    } else if m.contains("remote has disconnected") && m.contains("shut down") {
        "Die Konsole wurde ausgeschaltet."
    } else if m.contains("remote has disconnected") {
        "Die Konsole hat die Verbindung getrennt."
    } else if m.contains("stopped") {
        "Beendet."
    } else {
        return msg.to_string();
    };
    format!("{t}
({msg})")
}

fn setup_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = th_bg();
    v.window_fill = th_card();
    v.override_text_color = Some(Color32::from_rgb(236, 241, 255));
    v.widgets.inactive.bg_fill = th_card_hover();
    v.widgets.hovered.bg_fill = th_card_hover();
    v.widgets.hovered.expansion = 1.5;
    v.widgets.hovered.bg_stroke = Stroke::new(1.2, th_blue_light().gamma_multiply(0.6));
    v.widgets.active.expansion = -0.5;
    v.selection.bg_fill = th_blue();
    ctx.set_visuals(v);
    let mut s = (*ctx.style()).clone();
    s.spacing.item_spacing = Vec2::new(10.0, 10.0);
    s.spacing.button_padding = Vec2::new(18.0, 9.0);
    s.spacing.slider_width = 260.0;
    s.animation_time = 0.14;
    ctx.set_style(s);
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

/// Speichert das letzte Bild als PNG in Bilder\OVERLIME. Gibt den Dateinamen zurück.
fn save_screenshot(buf: &[u8], w: usize, h: usize) -> Result<String, String> {
    if w == 0 || h == 0 || buf.len() < w * h * 4 {
        return Err("Kein Bild".into());
    }
    let base = std::env::var("USERPROFILE").map_err(|e| e.to_string())?;
    let dir = std::path::PathBuf::from(base).join("Pictures").join("OVERLIME");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name = format!("OVERLIME-{secs}.png");
    let file = std::fs::File::create(dir.join(&name)).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(&buf[..w * h * 4]).map_err(|e| e.to_string())?;
    Ok(name)
}

/// Zielpfad für eine Aufnahme: Videos\OVERLIME\OVERLIME-<Zeit>.mp4 (voller Pfad, Dateiname).
fn recording_path() -> Result<(String, String), String> {
    let base = std::env::var("USERPROFILE").map_err(|e| e.to_string())?;
    let dir = std::path::PathBuf::from(base).join("Videos").join("OVERLIME");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name = format!("OVERLIME-{secs}.mp4");
    Ok((dir.join(&name).to_string_lossy().to_string(), name))
}

/// Leiser Testton (440 Hz) für die Geräteauswahl.
fn tone_pull() -> audio::Pull {
    let n = Arc::new(std::sync::atomic::AtomicU64::new(0));
    Arc::new(move |buf: &mut [i16]| {
        let frames = buf.len() / 2;
        let start = n.fetch_add(frames as u64, std::sync::atomic::Ordering::Relaxed);
        for i in 0..frames {
            let v = ((2.0 * std::f64::consts::PI * 440.0 * (start + i as u64) as f64 / 48000.0).sin() * 6000.0) as i16;
            buf[i * 2] = v;
            buf[i * 2 + 1] = v;
        }
        frames * 2
    })
}

struct BarStats {
    update: Option<String>,
    ping: f32,
    dec: f32,
    mbit: f32,
    loss: f32,
    pad: String,
}

#[derive(Default)]
struct BarFlags {
    quick: bool,
    menu: bool,
    fullscreen: bool,
    keys: bool,
    shot: bool,
    mute: bool,
    rec: bool,
}

/// Elegante Leiste im Stream: Verbindungsstatus links, Schnellzugriffe rechts.
fn draw_stream_bar(ctx: &egui::Context, anim: f32, t: f64, ps5: bool, game: &str, quality: &str, no_audio: Option<&str>, muted: bool, volume: &mut f32, stats: &BarStats) -> BarFlags {
    let mut fl = BarFlags::default();
    let w = ctx.screen_rect().width();
    let console_txt = if ps5 { "PlayStation 5" } else { "PlayStation 4" };
    let pulse = 0.6 + 0.4 * (t * 2.2).sin().abs() as f32;
    egui::Area::new(egui::Id::new("bar")).order(egui::Order::Foreground).fixed_pos(Pos2::new(0.0, -(1.0 - anim) * 60.0)).show(ctx, |ui| {
        ui.set_opacity(anim);
        egui::Frame::none()
            .fill(Color32::from_rgba_unmultiplied(8, 11, 22, 228))
            .inner_margin(egui::Margin::symmetric(18.0, 9.0))
            .show(ui, |ui| {
                ui.set_width(w - 36.0);
                ui.horizontal_centered(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 7.0 * pulse, GREEN.gamma_multiply(0.25));
                    ui.painter().circle_filled(r.center(), 4.0, GREEN);
                    ui.label(RichText::new("Verbunden").size(14.0).strong().color(GREEN));
                    ui.label(RichText::new(console_txt).size(14.0));
                    if !game.is_empty() && w >= 1000.0 {
                        ui.label(RichText::new(format!("•  {game}")).size(13.0).color(TEXT_DIM));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().button_padding = Vec2::new(4.0, 3.0);
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let ib = |icon: &str| egui::Button::new(RichText::new(icon.to_string()).size(17.0)).fill(Color32::from_white_alpha(14)).rounding(9.0).min_size(Vec2::new(36.0, 30.0));
                        if ui.add(ib(ic::POWER)).on_hover_text("Menü / Beenden (Esc)").clicked() {
                            fl.menu = true;
                        }
                        if ui.add(ib(ic::CORNERS_OUT)).on_hover_text("Vollbild (F11)").clicked() {
                            fl.fullscreen = true;
                        }
                        if ui.add(ib(ic::SLIDERS)).on_hover_text("Schnell-Einstellungen: Schärfe, Qualität").clicked() {
                            fl.quick = true;
                        }
                        if ui.add(ib(ic::KEYBOARD)).on_hover_text("Tastenbelegung (F1)").clicked() {
                            fl.keys = true;
                        }
                        if ui.add(ib(ic::CAMERA)).on_hover_text("Screenshot (F12)").clicked() {
                            fl.shot = true;
                        }
                        if ui.add(ib(ic::RECORD)).on_hover_text("Aufnahme starten/beenden (F9)").clicked() {
                            fl.rec = true;
                        }
                        ui.scope(|ui| {
                            ui.spacing_mut().slider_width = 96.0;
                            ui.add(egui::Slider::new(volume, 0.0..=1.5).show_value(false));
                        });
                        let mute_icon = if muted { ic::SPEAKER_X } else { ic::SPEAKER_HIGH };
                        if ui.add(ib(mute_icon)).on_hover_text("Ton an/aus (M)").clicked() {
                            fl.mute = true;
                        }
                        if let Some(why) = no_audio {
                            ui.label(RichText::new("Kein Ton").size(12.5).color(YELLOW)).on_hover_text(why.to_string());
                        }
                        if w >= 860.0 {
                            ui.label(RichText::new(quality).size(12.5).color(TEXT_DIM));
                        }
                        if let Some(v) = &stats.update {
                            if w >= 900.0 {
                                ui.label(RichText::new(format!("{}  Update {v} bereit", ic::ARROW_CIRCLE_UP)).size(12.5).color(YELLOW))
                                    .on_hover_text("Es wird nie während des Spiels installiert. Nach dem Spiel in den Einstellungen unter „Updates“ aktualisieren.");
                            }
                        }
                        if stats.ping >= 0.0 && w >= 760.0 {
                            let qcol = if stats.ping < 40.0 && stats.loss < 0.5 { GREEN } else if stats.ping < 90.0 && stats.loss < 3.0 { YELLOW } else { RED };
                            let total = stats.ping + stats.dec + 8.0;
                            let tip = format!(
                                "Verbindung\nPing zur Konsole: {:.0} ms\nDekodierung: {:.1} ms\nGesamt ca.: {:.0} ms (Ping + Dekodierung + ein Bild)\nDatenrate: {:.1} Mbit/s\nVerluste: {:.1} pro Sekunde",
                                stats.ping, stats.dec, total, stats.mbit, stats.loss
                            );
                            let label = if w >= 1100.0 { format!("{}  {:.0} ms • {:.1} Mbit/s", ic::WIFI_HIGH, total, stats.mbit) } else { format!("{}  {:.0} ms", ic::WIFI_HIGH, total) };
                            ui.label(RichText::new(label).size(12.5).color(qcol)).on_hover_text(tip);
                        }
                        if !stats.pad.is_empty() && w >= 1000.0 {
                            ui.label(RichText::new(format!("{}  {}", ic::GAME_CONTROLLER, stats.pad)).size(12.5).color(TEXT_DIM));
                        }
                    });
                });
            });
    });
    fl
}

fn ease_out_t(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

fn fade(t: f64, since: f64, dur: f64) -> f32 {
    (((t - since) / dur).clamp(0.0, 1.0) as f32).powf(0.6)
}

fn big_button(text: &str, fill: Color32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).size(16.0).strong())
        .fill(fill)
        .rounding(12.0)
        .min_size(Vec2::new(160.0, 46.0))
}

/// Zeichnet eine Konsole (PS5 weiß/geschwungen, PS4 schwarz) mit pulsierendem Schein. k = Größe (1.0 = 130 px).
pub fn paint_console(p: &egui::Painter, c: Pos2, k: f32, ps5: bool, on: bool, t: f64, hover: f32) {
    let pt = |dx: f32, dy: f32| Pos2::new(c.x + dx * k, c.y + dy * k);
    let pulse = if on { 0.5 + 0.5 * (t * 2.0).sin() as f32 } else { 0.0 };
    let glow = if on { th_blue() } else { Color32::from_rgb(60, 70, 100) };
    p.circle_filled(c, (60.0 + 3.0 * pulse + 3.0 * hover) * k, glow.gamma_multiply(0.10 + 0.06 * pulse));
    p.circle_filled(c, 44.0 * k, glow.gamma_multiply(0.12));
    let led = if on { th_blue_light().gamma_multiply(0.65 + 0.35 * pulse) } else { Color32::from_rgb(90, 98, 120) };

    if ps5 {
        let w = Color32::from_rgb(242, 244, 250);
        let w2 = Color32::from_rgb(207, 212, 228);
        p.add(Shape::convex_polygon(vec![pt(-6.0, -52.0), pt(-28.0, -40.0), pt(-30.0, 44.0), pt(-6.0, 54.0)], w, Stroke::NONE));
        p.add(Shape::convex_polygon(vec![pt(6.0, -52.0), pt(28.0, -40.0), pt(30.0, 44.0), pt(6.0, 54.0)], w2, Stroke::NONE));
        p.rect_filled(Rect::from_center_size(c, Vec2::new(13.0, 104.0) * k), Rounding::same(6.0 * k), Color32::from_rgb(18, 20, 28));
        p.rect_filled(Rect::from_center_size(pt(0.0, -20.0), Vec2::new(3.0, 40.0) * k), Rounding::same(2.0 * k), led);
    } else {
        p.add(Shape::convex_polygon(vec![pt(-30.0, -40.0), pt(52.0, -40.0), pt(30.0, 40.0), pt(-52.0, 40.0)], Color32::from_rgb(28, 31, 42), Stroke::new(1.5, Color32::from_rgb(70, 76, 96))));
        p.add(Shape::convex_polygon(vec![pt(-30.0, -40.0), pt(52.0, -40.0), pt(46.0, -20.0), pt(-36.0, -20.0)], Color32::from_rgb(40, 44, 58), Stroke::NONE));
        p.rect_filled(Rect::from_center_size(pt(-4.0, 10.0), Vec2::new(42.0, 4.0) * k), Rounding::same(2.0 * k), led);
    }
}

fn draw_console(ui: &mut egui::Ui, ps5: bool, on: bool, t: f64, hover: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(130.0, 130.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    paint_console(&p, rect.center(), 1.0, ps5, on, t, hover);
}

/// Platzhalter-Karte mit wanderndem Glanz, solange gesucht wird.
fn skeleton_card(ui: &mut egui::Ui, t: f64, phase: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 130.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, Rounding::same(18.0), th_card());
    let block = |x: f32, y: f32, w: f32, h: f32, r: f32| {
        p.rect_filled(Rect::from_min_size(rect.min + Vec2::new(x, y), Vec2::new(w, h)), Rounding::same(r), Color32::from_white_alpha(12));
    };
    p.circle_filled(rect.min + Vec2::new(83.0, 65.0), 48.0, Color32::from_white_alpha(10));
    block(160.0, 28.0, 120.0, 12.0, 6.0);
    block(160.0, 52.0, 240.0, 22.0, 8.0);
    block(160.0, 86.0, 170.0, 12.0, 6.0);
    block(rect.width() - 190.0, 42.0, 160.0, 46.0, 12.0);
    // Glanz
    let x = ((t as f32 * 0.55 + phase) % 1.6 - 0.3) * rect.width();
    let shine = Rect::from_min_max(Pos2::new((rect.min.x + x).max(rect.min.x), rect.min.y), Pos2::new((rect.min.x + x + 140.0).min(rect.max.x), rect.max.y));
    if shine.width() > 1.0 {
        p.rect_filled(shine, Rounding::same(18.0), Color32::from_white_alpha(7));
    }
}

/// Kleiner Status-Chip für die Kopfleiste.
fn top_chip(ui: &mut egui::Ui, icon: &str, text: &str, color: Color32) {
    let g_icon = ui.painter().layout_no_wrap(icon.to_string(), FontId::proportional(14.0), color);
    let g_text = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(12.5), Color32::from_rgb(220, 228, 245));
    let size = Vec2::new(g_icon.size().x + g_text.size().x + 32.0, 28.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(rect, Rounding::same(14.0), color.gamma_multiply(0.13));
    ui.painter().rect_stroke(rect, Rounding::same(14.0), Stroke::new(1.0, color.gamma_multiply(0.35)));
    let cy = rect.center().y;
    ui.painter().galley(Pos2::new(rect.min.x + 11.0, cy - g_icon.size().y / 2.0), g_icon.clone(), color);
    ui.painter().galley(Pos2::new(rect.min.x + 11.0 + g_icon.size().x + 6.0, cy - g_text.size().y / 2.0), g_text, Color32::WHITE);
}

fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(13.0), color);
    let size = galley.size() + Vec2::new(28.0, 10.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(rect, Rounding::same(size.y / 2.0), color.gamma_multiply(0.15));
    ui.painter().circle_filled(Pos2::new(rect.min.x + 12.0, rect.center().y), 3.5, color);
    ui.painter().galley(Pos2::new(rect.min.x + 22.0, rect.center().y - galley.size().y / 2.0), galley, color);
}

pub const HUD_ITEMS: [&str; 10] = ["Auflösung", "FPS", "Ping", "Dekodierzeit", "Datenrate", "Paketverlust", "Controller", "Ton-Status", "Uhrzeit", "Akku"];

/// Auswahl, was die Statusanzeige im Stream zeigt, und wo sie steht.
fn hud_config_ui(ui: &mut egui::Ui, st: &mut Settings) {
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
        for (i, name) in HUD_ITEMS.iter().enumerate() {
            let on = st.hud_items & (1 << i) != 0;
            if chip_toggle(ui, name, on) {
                st.hud_items ^= 1 << i;
            }
        }
    });
    ui.add_space(8.0);
    ui.label(RichText::new("Position").size(13.0).color(TEXT_DIM));
    segmented(ui, &mut st.hud_pos, &["Oben links", "Oben rechts", "Unten links", "Unten rechts"]);
    ui.add_space(6.0);
    ui.label(RichText::new("Größe").size(13.0).color(TEXT_DIM));
    segmented(ui, &mut st.hud_size, &["Klein", "Normal", "Groß"]);
    ui.add_space(6.0);
    if chip_toggle(ui, "Mit Hintergrund", st.hud_box) {
        st.hud_box = !st.hud_box;
    }
}

/// Zeichnet die Statusanzeige im Stream an die gewählte Stelle.
fn paint_hud(painter: &egui::Painter, full: Rect, hud: String, st: &Settings, bar_shift: f32) {
    paint_hud_scaled(painter, full, hud, st, bar_shift, 1.0, None);
}

/// Wie `paint_hud`, aber skaliert (für die Vorschau) und mit weicher Bewegung beim Wechsel der Position.
fn paint_hud_scaled(painter: &egui::Painter, full: Rect, hud: String, st: &Settings, bar_shift: f32, scale: f32, anim: Option<&egui::Context>) {
    let base = [11.0, 12.5, 15.0][st.hud_size.min(2)];
    let margin = 14.0 * scale;
    let pad_box = if st.hud_box { 9.0 * scale } else { 0.0 };
    let alpha = if st.hud_box { 235 } else { 190 };
    let mut size = base * scale;
    let mut galley = painter.layout_no_wrap(hud.clone(), FontId::monospace(size), Color32::from_white_alpha(alpha));
    // zu lang für die Vorschau: Schrift verkleinern, bis es passt
    let max_w = full.width() - 2.0 * (margin + pad_box);
    if galley.size().x > max_w && max_w > 40.0 {
        size *= 0.94 * max_w / galley.size().x;
        galley = painter.layout_no_wrap(hud, FontId::monospace(size), Color32::from_white_alpha(alpha));
    }
    let (w, h) = (galley.size().x + 2.0 * pad_box, galley.size().y + 2.0 * pad_box);
    let top = st.hud_pos < 2;
    let left = st.hud_pos % 2 == 0;
    let mut x = if left { full.left() + margin } else { full.right() - margin - w };
    let mut y = if top { full.top() + 12.0 * scale + bar_shift } else { full.bottom() - 12.0 * scale - h };
    if let Some(ctx) = anim {
        x = ctx.animate_value_with_time(egui::Id::new("hud_prev_x"), x, 0.3);
        y = ctx.animate_value_with_time(egui::Id::new("hud_prev_y"), y, 0.3);
    }
    let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
    if st.hud_box {
        painter.rect_filled(r, Rounding::same(10.0 * scale.max(0.6)), Color32::from_black_alpha(150));
    } else {
        // ohne Hintergrund: dezenter Schatten für die Lesbarkeit
        painter.rect_filled(r.expand(4.0 * scale), Rounding::same(6.0), Color32::from_black_alpha(60));
    }
    painter.galley(r.min + Vec2::splat(pad_box), galley, Color32::WHITE);
}

/// Große Kachel mit Symbol und Beschriftung (für das Pause-Menü).
fn tile(ui: &mut egui::Ui, icon: &str, label: &str, w: f32, active: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 78.0), egui::Sense::click());
    let h = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let base = if active { RED.gamma_multiply(0.45) } else { th_card_hover() };
    let fill = base.lerp_to_gamma(th_blue().gamma_multiply(0.7), h);
    ui.painter().rect_filled(rect, Rounding::same(14.0), fill);
    ui.painter().rect_stroke(rect, Rounding::same(14.0), Stroke::new(1.0, th_blue_light().gamma_multiply(0.35 * h)));
    let lift = -2.0 * h;
    ui.painter().text(rect.center() + Vec2::new(0.0, -10.0 + lift), Align2::CENTER_CENTER, icon, FontId::proportional(24.0), if active { RED } else { Color32::WHITE });
    ui.painter().text(rect.center() + Vec2::new(0.0, 20.0 + lift), Align2::CENTER_CENTER, label, FontId::proportional(12.5), Color32::from_gray(190).lerp_to_gamma(Color32::WHITE, h));
    resp.clicked()
}

/// Abgesetzter Bereich mit Überschrift.
fn card(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none().fill(Color32::from_white_alpha(9)).rounding(14.0).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong().size(13.5));
        ui.add_space(4.0);
        add(ui);
    });
}

/// Kleine Anzeige (Wert mit Farbe).
fn stat_chip(ui: &mut egui::Ui, text: &str, col: Color32) {
    egui::Frame::none().fill(col.gamma_multiply(0.18)).rounding(10.0).stroke(Stroke::new(1.0, col.gamma_multiply(0.45))).inner_margin(egui::Margin::symmetric(9.0, 3.0)).show(ui, |ui| {
        ui.label(RichText::new(text).size(12.0).color(col));
    });
}

/// Kleines Liniendiagramm (z. B. Ping über die letzte Minute).
fn sparkline(ui: &mut egui::Ui, data: &[f32], color: Color32, min_top: f32, unit: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 54.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, Rounding::same(10.0), Color32::from_white_alpha(10));
    if data.len() < 2 {
        p.text(rect.center(), Align2::CENTER_CENTER, "Sammelt Messwerte …", FontId::proportional(12.0), TEXT_DIM);
        return;
    }
    let top = data.iter().cloned().fold(min_top, f32::max);
    let n = data.len();
    let pts: Vec<Pos2> = data
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.min.x + 6.0 + (rect.width() - 12.0) * i as f32 / (n - 1) as f32;
            let y = rect.max.y - 6.0 - (rect.height() - 12.0) * (v / top).clamp(0.0, 1.0);
            Pos2::new(x, y)
        })
        .collect();
    let mut fill = pts.clone();
    fill.push(Pos2::new(pts[n - 1].x, rect.max.y - 6.0));
    fill.push(Pos2::new(pts[0].x, rect.max.y - 6.0));
    p.add(egui::Shape::convex_polygon(fill, color.gamma_multiply(0.12), Stroke::NONE));
    p.add(egui::Shape::line(pts, Stroke::new(1.8, color)));
    let last = data[n - 1];
    p.text(Pos2::new(rect.max.x - 8.0, rect.min.y + 9.0), Align2::RIGHT_CENTER, format!("{:.0} {unit}", last), FontId::proportional(11.5), Color32::WHITE);
    p.text(Pos2::new(rect.min.x + 8.0, rect.min.y + 9.0), Align2::LEFT_CENTER, format!("max {:.0}", top), FontId::proportional(10.5), TEXT_DIM);
}

/// Auswahlleiste: flache Fläche, die Auswahl gleitet weich zur neuen Stelle.
fn segmented(ui: &mut egui::Ui, value: &mut usize, labels: &[&str]) {
    if labels.is_empty() {
        return;
    }
    let ctx = ui.ctx().clone();
    let painter = ui.painter().clone();
    let avail = ui.available_width();
    let mut font = 14.0;
    let texts: Vec<String> = labels.iter().map(|l| lang::tr(l)).collect();
    let measure = |font: f32, pad: f32, min: f32| -> Vec<f32> { texts.iter().map(|t| (painter.layout_no_wrap(t.clone(), FontId::proportional(font), Color32::WHITE).size().x + pad).max(min)).collect() };
    let mut widths = measure(font, 36.0, 76.0);
    let mut total: f32 = widths.iter().sum::<f32>() + 6.0;
    if total > avail {
        font = 12.5;
        widths = measure(font, 20.0, 48.0);
        total = widths.iter().sum::<f32>() + 6.0;
        if total > avail {
            let k = (avail - 6.0) / (total - 6.0);
            for w in widths.iter_mut() {
                *w *= k;
            }
            total = avail;
        }
    }
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total, 40.0), egui::Sense::hover());
    let id = ui.id().with(("seg", labels.join("|")));
    painter.rect_filled(rect, Rounding::same(10.0), Color32::from_rgba_unmultiplied(255, 255, 255, 9));
    let mut x = rect.left() + 3.0;
    let mut rects = Vec::with_capacity(widths.len());
    for w in &widths {
        rects.push(Rect::from_min_size(Pos2::new(x, rect.top() + 3.0), Vec2::new(*w, 34.0)));
        x += *w;
    }
    let sel = (*value).min(rects.len() - 1);
    let ax = ctx.animate_value_with_time(id.with("x"), rects[sel].left() - rect.left(), 0.2) + rect.left();
    let aw = ctx.animate_value_with_time(id.with("w"), rects[sel].width(), 0.2);
    let pill = Rect::from_min_size(Pos2::new(ax, rect.top() + 3.0), Vec2::new(aw, 34.0));
    painter.rect_filled(pill, Rounding::same(8.0), th_blue());
    let mut clicked = None;
    for (i, r) in rects.iter().enumerate() {
        let resp = ui.interact(*r, id.with(i), egui::Sense::click());
        let hv = ctx.animate_bool_with_time(id.with(("h", i)), resp.hovered() && i != sel, 0.12);
        if hv > 0.01 {
            painter.rect_filled(*r, Rounding::same(8.0), Color32::from_white_alpha((14.0 * hv) as u8));
        }
        let on = (pill.center().x - r.center().x).abs() < r.width() * 0.5;
        let col = if on { Color32::WHITE } else { TEXT_DIM.lerp_to_gamma(Color32::WHITE, hv * 0.8) };
        painter.text(r.center(), Align2::CENTER_CENTER, labels[i], FontId::proportional(font), col);
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    if let Some(i) = clicked {
        *value = i;
    }
}

/// Schlichter Auswahl-Chip: eingeschaltet gefüllt in Akzentfarbe, sonst dezent.
fn chip_toggle(ui: &mut egui::Ui, name: &str, on: bool) -> bool {
    let ctx = ui.ctx().clone();
    let painter = ui.painter().clone();
    let text = lang::tr(name);
    let g = painter.layout_no_wrap(text.clone(), FontId::proportional(13.5), Color32::WHITE);
    let w = g.size().x + 40.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 36.0), egui::Sense::click());
    let id = resp.id;
    let a_on = ctx.animate_bool_with_time(id.with("on"), on, 0.16);
    let hv = ctx.animate_bool_with_time(id.with("hv"), resp.hovered(), 0.12);
    let base = Color32::from_rgba_unmultiplied(255, 255, 255, 10);
    painter.rect_filled(rect, Rounding::same(10.0), base.lerp_to_gamma(th_blue(), a_on));
    if hv > 0.01 {
        painter.rect_filled(rect, Rounding::same(10.0), Color32::from_white_alpha((12.0 * hv) as u8));
    }
    // kleiner Haken, nur wenn eingeschaltet
    let ic = Pos2::new(rect.left() + 17.0, rect.center().y);
    if a_on > 0.02 {
        splash::tick(&painter, ic, 10.0, Color32::WHITE.gamma_multiply(a_on), a_on);
    }
    painter.text(Pos2::new(rect.left() + 30.0, rect.center().y), Align2::LEFT_CENTER, text, FontId::proportional(13.5), TEXT_DIM.lerp_to_gamma(Color32::WHITE, (a_on + hv * 0.5).min(1.0)));
    resp.clicked()
}

/// Schalter mit Titel und Erklärung (zum Beispiel für die Zwischenbilder). Flach, ohne Leuchten.
fn fancy_switch(ui: &mut egui::Ui, value: &mut bool, title: &str, desc: &str, badge: &str) {
    let ctx = ui.ctx().clone();
    let width = ui.available_width();
    let id = ui.id().with(("fsw", title));
    let a_on = ctx.animate_bool_with_time(id.with("on"), *value, 0.18);
    let frame = egui::Frame::none().fill(Color32::from_rgba_unmultiplied(255, 255, 255, 8)).rounding(12.0).inner_margin(egui::Margin::symmetric(16.0, 12.0));
    let inner = frame.show(ui, |ui| {
        ui.set_width(width - 32.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(width - 32.0 - 64.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).size(15.0).strong());
                    if !badge.is_empty() {
                        ui.label(RichText::new(badge).size(12.0).color(YELLOW));
                    }
                });
                if !desc.is_empty() {
                    ui.add(egui::Label::new(RichText::new(desc).size(12.5).color(TEXT_DIM)).wrap());
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::new(46.0, 26.0), egui::Sense::hover());
                let p = ui.painter();
                p.rect_filled(r, Rounding::same(13.0), Color32::from_white_alpha(30).lerp_to_gamma(th_blue(), a_on));
                let kx = r.left() + 13.0 + (r.width() - 26.0) * a_on;
                p.circle_filled(Pos2::new(kx, r.center().y), 9.5, Color32::WHITE);
            });
        });
    });
    let resp = ui.interact(inner.response.rect, id, egui::Sense::click());
    let hv = ctx.animate_bool_with_time(id.with("hv"), resp.hovered(), 0.12);
    if hv > 0.01 {
        ui.painter().rect_filled(inner.response.rect, Rounding::same(12.0), Color32::from_white_alpha((8.0 * hv) as u8));
    }
    if resp.clicked() {
        *value = !*value;
    }
}

/// Vorschau der Statusanzeige: ein schlichter Bildschirm, die Anzeige liegt an der gewählten Ecke.
fn hud_preview(ui: &mut egui::Ui, st: &Settings) {
    let ctx = ui.ctx().clone();
    let w = ui.available_width().min(520.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, w * 9.0 / 16.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    // ruhiger Hintergrund mit leichtem Verlauf
    let mut m = egui::epaint::Mesh::default();
    let (c_top, c_bot) = (Color32::from_rgb(26, 31, 48), Color32::from_rgb(14, 17, 28));
    m.colored_vertex(rect.left_top(), c_top);
    m.colored_vertex(rect.right_top(), c_top);
    m.colored_vertex(rect.left_bottom(), c_bot);
    m.colored_vertex(rect.right_bottom(), c_bot);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 3, 2);
    p.add(Shape::mesh(m));
    p.rect_stroke(rect, Rounding::ZERO, Stroke::new(1.0, Color32::from_white_alpha(22)));
    p.text(rect.center(), Align2::CENTER_CENTER, lang::tr("Spielbild"), FontId::proportional(13.0), Color32::from_white_alpha(40));

    let demo = ["1920x1080", "60 FPS", "Ping 11 ms", "Dek. 3.2 ms", "27.4 Mbit/s", "4 Pakete verloren", "Controller", "Ton ok", "21:45", "Akku voll"];
    let parts: Vec<String> = demo.iter().enumerate().filter(|(i, _)| st.hud_items & (1 << i) != 0).map(|(_, s)| lang::tr(s)).collect();
    if parts.is_empty() {
        p.text(rect.center() + Vec2::new(0.0, 22.0), Align2::CENTER_CENTER, lang::tr("Nichts ausgewählt"), FontId::proportional(13.0), Color32::from_white_alpha(120));
        return;
    }
    // Die Vorschau zeigt die Anzeige größer als im echten Bild und bricht lange Zeilen um, damit alles lesbar bleibt.
    let size = [11.0, 12.5, 15.0][st.hud_size.min(2)] * 0.92;
    let pad = if st.hud_box { 8.0 } else { 0.0 };
    let margin = 12.0;
    let g = p.layout(parts.join("  •  "), FontId::monospace(size), Color32::from_white_alpha(if st.hud_box { 235 } else { 200 }), (w * 0.62).max(120.0));
    let (bw, bh) = (g.size().x + 2.0 * pad, g.size().y + 2.0 * pad);
    // Bewegung in Anteilen der Vorschau (nicht in Bildschirm-Pixeln), damit sie beim Scrollen nicht mitwandert
    let fx = ctx.animate_value_with_time(egui::Id::new("hud_prev_fx"), if st.hud_pos % 2 == 0 { 0.0 } else { 1.0 }, 0.25);
    let fy = ctx.animate_value_with_time(egui::Id::new("hud_prev_fy"), if st.hud_pos < 2 { 0.0 } else { 1.0 }, 0.25);
    let x = rect.left() + margin + (rect.width() - 2.0 * margin - bw) * fx;
    let y = rect.top() + margin + (rect.height() - 2.0 * margin - bh) * fy;
    let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(bw, bh));
    if st.hud_box {
        p.rect_filled(r, Rounding::same(8.0), Color32::from_black_alpha(160));
    }
    p.galley(r.min + Vec2::splat(pad), g, Color32::WHITE);
}

thread_local! {
    static SET_FILTER: std::cell::RefCell<Option<(usize, String)>> = const { std::cell::RefCell::new(None) };
    static SET_DRAWN: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Kategorie (1 Bild & Ton, 2 Steuerung, 3 Darstellung, 4 Verbindung, 5 App & Updates) und Suchbegriffe je Abschnitt.
fn section_meta(title: &str) -> (usize, &'static str) {
    let t = title.to_lowercase();
    let table: [(&str, usize, &str); 12] = [
        ("statusanzeige", 3, "hud anzeige ping fps position oben links statistik einblendung uhrzeit"),
        ("aufnahmen", 1, "aufnahme video screenshot ordner mp4 aufnehmen"),
        ("videoqualität", 1, "auflösung bildrate fps bitrate codec hevc h.264 schärfe qualität 1080p zwischenbilder 120 hz interpolation flüssig"),
        ("ton", 1, "ton lautstärke audio lautsprecher sound"),
        ("steuerung", 2, "controller vibration touchpad gyro bewegung trigger led gamepad"),
        ("tastatur", 2, "tastatur tasten belegung keyboard"),
        ("darstellung", 3, "größe skalierung fenster vollbild anzeige kompakt"),
        ("farbschema", 3, "farbe theme design themes schema"),
        ("verbindung", 4, "ip adresse konto abmelden playstation psn internet vpn tailscale"),
        ("discord", 5, "discord status presence"),
        ("updates", 5, "update version aktualisieren installieren suchen"),
        ("symbol neben der uhr", 5, "symbol tray uhr windows start autostart hintergrund wecken"),
    ];
    for (k, c, kw) in table {
        if t.contains(k) {
            return (c, kw);
        }
    }
    (0, "")
}

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let filter = SET_FILTER.with(|f| f.borrow().clone());
    if let Some((cat, q)) = filter {
        let (c, kw) = section_meta(title);
        let ok = if !q.is_empty() { title.to_lowercase().contains(&q) || kw.contains(&q) } else { cat == 0 || c == cat };
        if !ok {
            return;
        }
        SET_DRAWN.with(|d| d.set(d.get() + 1));
    }
    egui::Frame::none().fill(th_card()).rounding(16.0).inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).size(17.0).strong());
        ui.add_space(4.0);
        add(ui);
    });
    if SET_FILTER.with(|f| f.borrow().is_some()) {
        ui.add_space(8.0);
    }
}

fn label(ui: &mut egui::Ui, title: &str, desc: &str) {
    ui.label(RichText::new(title).size(14.0).strong());
    if !desc.is_empty() {
        ui.label(RichText::new(desc).size(12.5).color(TEXT_DIM));
    }
}

fn nav_item(ui: &mut egui::Ui, icon: &str, text: &str, selected: bool, badge: Option<String>) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), egui::Sense::click());
    let h = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    if !selected && h > 0.01 {
        ui.painter().rect_filled(rect, Rounding::same(12.0), th_card_hover().gamma_multiply(0.45 * h));
    }
    let col = if selected { Color32::WHITE } else { TEXT_DIM.lerp_to_gamma(Color32::WHITE, h) };
    let ic_col = if selected { th_blue_light() } else { col };
    ui.painter().text(rect.left_center() + Vec2::new(18.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(18.0), ic_col);
    ui.painter().text(rect.left_center() + Vec2::new(48.0, 0.0), Align2::LEFT_CENTER, text, FontId::proportional(15.0), col);
    if let Some(b) = badge {
        let c = rect.right_center() - Vec2::new(16.0, 0.0);
        ui.painter().circle_filled(c, 9.0, th_blue());
        ui.painter().text(c, Align2::CENTER_CENTER, b, FontId::proportional(12.0), Color32::WHITE);
    }
    resp.clicked()
}

fn step_row(ui: &mut egui::Ui, n: &str, text: &str) {
    ui.horizontal_top(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(26.0), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), 13.0, th_blue().gamma_multiply(0.35));
        ui.painter().text(r.center(), Align2::CENTER_CENTER, n, FontId::proportional(14.0), Color32::WHITE);
        ui.add(egui::Label::new(RichText::new(text).size(14.5)).wrap());
    });
}

impl App {
    fn consoles_page(&mut self, ui: &mut egui::Ui, t: f64) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Deine Konsolen").size(26.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.searching {
                    ui.spinner();
                    ui.label(RichText::new("Suche …").color(TEXT_DIM));
                } else if ui.add(egui::Button::new(format!("{} Neu suchen", ic::ARROWS_CLOCKWISE)).fill(th_card()).rounding(10.0)).clicked() {
                    self.search();
                }
            });
        });
        ui.label(RichText::new("Konsolen im selben Netzwerk werden automatisch gefunden.").color(TEXT_DIM).size(13.0));
        ui.add_space(10.0);

        if let Some(e) = self.start_error.clone() {
            egui::Frame::none().fill(RED.gamma_multiply(0.15)).rounding(12.0).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Verbindung fehlgeschlagen").strong().color(RED));
                ui.label(RichText::new(e).color(TEXT_DIM));
            });
            ui.add_space(8.0);
        }

        let list = self.consoles.clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            if list.is_empty() && self.searching {
                for k in 0..2 {
                    skeleton_card(ui, t, k as f32 * 0.4);
                    ui.add_space(4.0);
                }
            }
            if list.is_empty() {
                egui::Frame::none().fill(th_card()).rounding(18.0).inner_margin(egui::Margin::same(34.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        if self.searching {
                            ui.spinner();
                        }
                        ui.add_space(6.0);
                        ui.label(RichText::new("Keine Konsole gefunden").size(20.0).strong());
                        ui.label(RichText::new("Schalte deine PS4/PS5 ein (oder in den Ruhemodus) und verbinde sie mit demselben Netzwerk wie diesen PC.").color(TEXT_DIM));
                    });
                });
            }
            for c in list {
                let first = *self.first_seen.entry(c.addr.clone()).or_insert(t);
                let alpha = fade(t, first, 0.45);
                let paired = self.paired_for(&c);
                ui.scope(|ui| {
                    ui.set_opacity(alpha);
                    let bg = ui.painter().add(Shape::Noop);
                    let on = c.state == State::Ready;
                    let inner = egui::Frame::none().inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
                        ui.set_width(ui.available_width() - 4.0);
                        let hover_id = egui::Id::new(("hover", &c.addr));
                        let hv = ui.ctx().animate_bool_with_time(hover_id, ui.rect_contains_pointer(ui.max_rect()), 0.15);
                        ui.horizontal(|ui| {
                            draw_console(ui, c.is_ps5, on, t, hv);
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                ui.add_space(8.0);
                                ui.label(RichText::new(if c.is_ps5 { "PlayStation 5" } else { "PlayStation 4" }).color(th_blue_light()).size(13.0).strong());
                                ui.label(RichText::new(&c.name).size(24.0).strong());
                                ui.label(RichText::new(format!("Systemsoftware {}", pretty_version(&c.system_version))).color(TEXT_DIM).size(13.0));
                                ui.horizontal(|ui| {
                                    match c.state {
                                        State::Ready => pill(ui, "Bereit", GREEN),
                                        State::Standby => pill(ui, "Ruhemodus", YELLOW),
                                        State::Unknown => pill(ui, "Unbekannt", TEXT_DIM),
                                    }
                                    if paired.is_some() {
                                        pill(ui, "Gekoppelt", th_blue_light());
                                    }
                                });
                                if !c.running_app.is_empty() {
                                    ui.label(RichText::new(format!("{} {}", ic::GAME_CONTROLLER, c.running_app)).color(TEXT_DIM).size(13.0));
                                }
                                if paired.is_some() {
                                    let idx = self.settings.profiles.iter().position(|(id, _)| *id == c.id);
                                    let mut on = idx.is_some();
                                    if ui.checkbox(&mut on, RichText::new("Eigene Qualität für diese Konsole").size(13.0).color(TEXT_DIM)).changed() {
                                        if on {
                                            self.settings.profiles.push((c.id.clone(), settings::Profile { resolution: self.settings.resolution, fps: self.settings.fps, bitrate: self.settings.bitrate, codec: self.settings.codec }));
                                        } else if let Some(i) = idx {
                                            self.settings.profiles.remove(i);
                                        }
                                    }
                                    if let Some(i) = self.settings.profiles.iter().position(|(id, _)| *id == c.id) {
                                        let pr = &mut self.settings.profiles[i].1;
                                        let summary = format!("{} • {} FPS • {} Mbit/s • {}", ["540p", "720p", "1080p"][pr.resolution], [30, 60][pr.fps], pr.bitrate as i32, ["Auto", "H.264", "HEVC"][pr.codec]);
                                        egui::CollapsingHeader::new(RichText::new(summary).size(13.0).color(th_blue_light())).id_source(("prof", i)).show(ui, |ui| {
                                            segmented(ui, &mut pr.resolution, &["540p", "720p", "1080p"]);
                                            segmented(ui, &mut pr.fps, &["30 FPS", "60 FPS"]);
                                            ui.spacing_mut().slider_width = 220.0;
                                            ui.add(egui::Slider::new(&mut pr.bitrate, 2.0..=50.0).suffix(" Mbit/s"));
                                            segmented(ui, &mut pr.codec, &["Auto", "H.264", "HEVC"]);
                                        });
                                    }
                                }
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match &paired {
                                None => {
                                    if ui.add(big_button("Koppeln", th_blue())).clicked() {
                                        self.open_dialog(&c);
                                    }
                                }
                                Some(p) if c.state == State::Standby => {
                                    if ui.add(big_button("Aufwecken", th_blue())).clicked() {
                                        self.wake(&c, p);
                                    }
                                }
                                Some(p) => {
                                    let busy = self.starting.is_some();
                                    let txt: &str = if busy { "Verbinde …" } else { &format!("{}  Verbinden", ic::PLAY) };
                                    if ui.add_enabled(!busy, big_button(txt, th_blue())).clicked() {
                                        self.connect(&c, p);
                                    }
                                }
                            });
                        });
                        hv
                    });
                    let hv = inner.inner;
                    let rect = inner.response.rect;
                    let fill = th_card().lerp_to_gamma(th_card_hover(), hv);
                    let stroke = Stroke::new(1.5, th_blue().gamma_multiply(0.7 * hv));
                    ui.painter().set(bg, RectShape::new(rect, Rounding::same(18.0), fill, stroke));
                });
                ui.add_space(4.0);
            }
            if !self.status.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(&self.status).color(TEXT_DIM));
            }
        });
    }

    fn settings_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Einstellungen").size(26.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let b = egui::Button::new(RichText::new(format!("{} Maximale Qualität", ic::LIGHTNING)).strong()).fill(th_blue()).rounding(10.0);
                if ui.add(b).clicked() {
                    self.settings.max_quality();
                }
            });
        });
        ui.label(RichText::new("Wird automatisch gespeichert.").color(TEXT_DIM).size(13.0));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            egui::Frame::none().fill(th_card()).rounding(10.0).inner_margin(egui::Margin::symmetric(10.0, 4.0)).show(ui, |ui| {
                ui.label(RichText::new(ic::MAGNIFYING_GLASS).color(TEXT_DIM));
                ui.add(egui::TextEdit::singleline(&mut self.set_q).frame(false).hint_text("Einstellung suchen …").desired_width(220.0));
                if !self.set_q.is_empty() && ui.add(egui::Button::new("x").frame(false)).clicked() {
                    self.set_q.clear();
                }
            });
        });
        ui.add_space(6.0);
        let searching = !self.set_q.trim().is_empty();
        ui.horizontal_wrapped(|ui| {
            let cats = ["Alle", "Bild & Ton", "Steuerung", "Darstellung", "Verbindung", "App & Updates"];
            for (i, c) in cats.iter().enumerate() {
                let sel = !searching && self.set_cat == i;
                let b = egui::Button::new(RichText::new(*c).size(13.5).color(if sel { Color32::WHITE } else { TEXT_DIM }))
                    .fill(if sel { th_blue() } else { th_card() })
                    .rounding(16.0)
                    .min_size(Vec2::new(0.0, 30.0));
                if ui.add(b).clicked() {
                    self.set_cat = i;
                    self.set_q.clear();
                }
            }
        });
        ui.add_space(8.0);
        SET_FILTER.with(|f| *f.borrow_mut() = Some((self.set_cat, self.set_q.trim().to_lowercase())));
        SET_DRAWN.with(|d| d.set(0));
        let st = &mut self.settings;
        let mut sa = egui::ScrollArea::vertical();
        if let Some(off) = std::env::args().find_map(|a| a.strip_prefix("--scroll=").and_then(|v| v.parse::<f32>().ok())) {
            sa = sa.vertical_scroll_offset(off);
        }
        sa.show(ui, |ui| {
            section(ui, &format!("{}  Updates", ic::ARROW_CIRCLE_UP), |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Installierte Version: {VERSION}")).size(14.5).strong());
                    if self.update.is_none() && self.update_checked {
                        pill(ui, "Aktuell", GREEN);
                    }
                });
                if let Some(rel) = self.update.clone() {
                    ui.add_space(4.0);
                    egui::Frame::none().fill(th_blue().gamma_multiply(0.18)).rounding(12.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.5))).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(format!("Version {} ist verfügbar", rel.version)).size(16.0).strong());
                        let hl = update::highlights(&rel.notes);
                        if !hl.is_empty() {
                            ui.add_space(4.0);
                            ui.label(RichText::new("Neu:").color(TEXT_DIM).size(12.5));
                            for h in hl {
                                ui.label(RichText::new(format!("•  {h}")).size(13.0));
                            }
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if self.update_busy {
                                ui.spinner();
                                ui.label(RichText::new("Wird installiert …").color(TEXT_DIM));
                            } else if ui.add(big_button("Jetzt aktualisieren", th_blue()).min_size(Vec2::new(200.0, 42.0))).clicked() {
                                self.update_click = true;
                            }
                            ui.label(RichText::new("OVERLIME startet danach neu.").color(TEXT_DIM).size(12.0));
                        });
                        if !self.update_err.is_empty() {
                            ui.label(RichText::new(&self.update_err).color(RED).size(12.5));
                        }
                    });
                } else {
                    ui.add_space(2.0);
                    let ago = self.update_checked_at.map(|t| t.elapsed().as_secs() / 60);
                    let txt = match ago {
                        Some(0) => "Zuletzt geprüft: gerade eben".to_string(),
                        Some(n) => format!("Zuletzt geprüft: vor {n} Min."),
                        None => "Noch nicht geprüft.".to_string(),
                    };
                    ui.label(RichText::new(txt).color(TEXT_DIM).size(12.5));
                }
                    ui.horizontal(|ui| {
                    if self.update_checking {
                        ui.spinner();
                        ui.label(RichText::new("Suche …").color(TEXT_DIM));
                    } else if ui.add(egui::Button::new(format!("{} Nach Updates suchen", ic::ARROWS_CLOCKWISE)).fill(th_card_hover()).rounding(10.0)).clicked() {
                        self.check_click = true;
                    }
                    if !self.update_msg.is_empty() {
                        ui.label(RichText::new(&self.update_msg).color(TEXT_DIM).size(12.5));
                    }
                });
                ui.add_space(4.0);
                ui.checkbox(&mut st.auto_update, "Beim Start automatisch installieren (während der App-Nutzung nur Hinweis, nie während eines Spiels)");
            });
            section(ui, &format!("{}  Symbol neben der Uhr", ic::POWER), |ui| {
                ui.label(RichText::new("Ein kleines Symbol unten rechts: Ein Klick weckt die PlayStation und öffnet OVERLIME direkt verbunden. Es braucht kaum Speicher und startet auf Wunsch mit Windows.").color(TEXT_DIM).size(12.5));
                let mut ta = self.tray_auto;
                if ui.checkbox(&mut ta, "Mit Windows starten (Symbol im Hintergrund)").changed() {
                    match tray::set_autostart(ta) {
                        Ok(()) => {
                            self.tray_auto = ta;
                            if ta {
                                tray::spawn_tray();
                                self.toasts.push(("Symbol neben der Uhr gestartet.".to_string(), 2, self.now));
                            }
                        }
                        Err(e) => self.toasts.push((format!("Nicht möglich: {e}"), 1, self.now)),
                    }
                }
                ui.add_space(6.0);
                ui.checkbox(&mut st.notify_ready, "Hinweis in Windows, wenn die Konsole bereit ist");
                ui.checkbox(&mut st.wake_on, "Konsole zu einer festen Zeit aufwecken");
                if st.wake_on {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Uhrzeit").size(13.0).color(TEXT_DIM));
                        let mut h = st.wake_hour as u32;
                        let mut mi = st.wake_min as u32;
                        ui.add(egui::DragValue::new(&mut h).range(0..=23).custom_formatter(|v, _| format!("{:02}", v as u32)));
                        ui.label(":");
                        ui.add(egui::DragValue::new(&mut mi).range(0..=59).custom_formatter(|v, _| format!("{:02}", v as u32)));
                        st.wake_hour = h as u8;
                        st.wake_min = mi as u8;
                    });
                    ui.horizontal(|ui| {
                        for (i, d) in ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"].iter().enumerate() {
                            let on = st.wake_days & (1 << i) != 0;
                            let b = egui::Button::new(RichText::new(*d).size(13.0).color(if on { Color32::WHITE } else { TEXT_DIM })).fill(if on { th_blue() } else { th_card_hover() }).rounding(8.0).min_size(Vec2::new(36.0, 28.0));
                            if ui.add(b).clicked() {
                                st.wake_days ^= 1 << i;
                            }
                        }
                    });
                    ui.label(RichText::new("Das klappt, solange das Symbol neben der Uhr läuft (Haken „Mit Windows starten“) und der PC an ist.").color(TEXT_DIM).size(12.0));
                }
                ui.add_space(6.0);
                if ui.add(egui::Button::new("Symbol jetzt starten").fill(th_card_hover()).rounding(10.0)).clicked() {
                    tray::spawn_tray();
                    self.toasts.push(("Symbol neben der Uhr gestartet (evtl. unter dem Pfeil ^).".to_string(), 0, self.now));
                }
                });
            section(ui, &format!("{}  Videoqualität", ic::FILM_STRIP), |ui| {
                label(ui, "Auflösung", "Sony erlaubt im Remote Play höchstens 1080p.");
                segmented(ui, &mut st.resolution, &["540p", "720p", "1080p"]);
                    label(ui, "Bildrate", "");
                segmented(ui, &mut st.fps, &["30 FPS", "60 FPS"]);
                ui.add_space(8.0);
                let hz = winui::display_refresh_hz();
                let badge = if hz > 0 && hz < 100 { "Dein Bildschirm hat nur 60 Hz" } else { "Experimentell" };
                fancy_switch(ui, &mut st.interp, "Zwischenbilder für 120 Hz", "Die Konsole sendet höchstens 60 Bilder pro Sekunde. Die Grafikkarte berechnet Bilder dazwischen, damit es auf einem Bildschirm mit 120 Hz oder mehr flüssiger wirkt. Kostet etwa 17 ms Verzögerung.", badge);
                ui.add_space(6.0);
                    label(ui, "Bitrate", "Höher = schärferes Bild, braucht aber eine schnelle Leitung.");
                ui.add(egui::Slider::new(&mut st.bitrate, 2.0..=50.0).suffix(" Mbit/s"));
                    label(ui, "Videocodec", "HEVC (H.265) ist bei gleicher Bitrate schärfer. Automatisch: HEVC bei der PS5, H.264 bei der PS4.");
                segmented(ui, &mut st.codec, &["Automatisch", "H.264", "HEVC"]);
            });
            section(ui, &format!("{}  Darstellung", ic::MONITOR), |ui| {
                label(ui, "Hochskalierung auf 4K", "Die Konsole sendet max. 1080p. Die Grafikkarte rechnet hoch: „Aus“ = harte Pixel, „Weich“ = glatt, „Scharf“ und „Ultra“ = Schärfefilter (CAS).");
                segmented(ui, &mut st.upscale, &["Aus", "Weich", "Scharf", "Ultra"]);
                ui.add_space(4.0);
                label(ui, "Sprache / Language", "");
                segmented(ui, &mut st.language, &["Deutsch", "English"]);
                ui.add_space(6.0);
                ui.add_space(4.0);
                    ui.checkbox(&mut st.fullscreen, "Vollbild beim Streamen");
                    label(ui, "Größe der Oberfläche", "Kompakt zeigt mehr auf einmal und wirkt aufgeräumter.");
                segmented(ui, &mut st.ui_scale, &["Kompakt", "Normal", "Groß"]);
            });
            section(ui, &format!("{}  Statusanzeige im Stream", ic::CHART_LINE), |ui| {
                ui.label(RichText::new("Wähle, was im Stream eingeblendet wird (an/aus mit F10), wo es steht und wie groß.").color(TEXT_DIM).size(12.5));
                ui.add_space(4.0);
                ui.checkbox(&mut self.show_hud, "Statusanzeige im Stream zeigen");
                ui.add_space(6.0);
                ui.label(RichText::new("Vorschau").size(13.0).color(TEXT_DIM));
                hud_preview(ui, st);
                ui.add_space(4.0);
                hud_config_ui(ui, st);
            });
            section(ui, &format!("{}  Farbschema", ic::PALETTE), |ui| {
                ui.label(RichText::new("Wähle ein Farbschema. Eigene Schemata legst du als Textdatei in den Themes-Ordner.").color(TEXT_DIM).size(12.5));
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for p in self.themes.iter() {
                        let sel = st.theme == p.name;
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(128.0, 74.0), egui::Sense::click());
                        let h = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                        let pr = ui.painter_at(rect);
                        pr.rect_filled(rect, Rounding::same(14.0), p.theme.bg);
                        pr.rect_filled(Rect::from_min_size(rect.min + Vec2::new(10.0, 10.0), Vec2::new(rect.width() - 20.0, 24.0)), Rounding::same(8.0), p.theme.card);
                        pr.rect_filled(Rect::from_min_size(rect.min + Vec2::new(10.0, 10.0), Vec2::new(34.0, 24.0)), Rounding::same(8.0), p.theme.accent);
                        pr.circle_filled(rect.min + Vec2::new(rect.width() - 24.0, 22.0), 6.0, p.theme.accent_light);
                        pr.text(rect.min + Vec2::new(12.0, 52.0), Align2::LEFT_CENTER, &p.name, FontId::proportional(12.5), Color32::WHITE);
                        let stroke = if sel { Stroke::new(2.2, p.theme.accent_light) } else { Stroke::new(1.0 + h, Color32::from_white_alpha((30.0 + 60.0 * h) as u8)) };
                        pr.rect_stroke(rect, Rounding::same(14.0), stroke);
                        if resp.clicked() {
                            st.theme = p.name.clone();
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new("Themes-Ordner öffnen").fill(th_card_hover()).rounding(10.0)).clicked() {
                        theme::ensure_example();
                        if let Some(d) = theme::themes_dir() {
                            let _ = std::process::Command::new("explorer").arg(d).spawn();
                        }
                    }
                    if ui.add(egui::Button::new("Themes neu laden").fill(th_card_hover()).rounding(10.0)).clicked() {
                        self.themes = theme::all();
                    }
                });
            });
            section(ui, &format!("{}  Ton", ic::SPEAKER_HIGH), |ui| {
                ui.label(RichText::new("Ausgabegerät und Lautstärke für den Ton aus dem Spiel. Ein anderes Gerät gilt ab dem nächsten Stream.").color(TEXT_DIM).size(12.5));
                ui.add_space(4.0);
                let std_name = audio::default_device_name();
                let sel_text = if st.audio_device.is_empty() { format!("Standardgerät ({std_name})") } else { st.audio_device.clone() };
                egui::ComboBox::from_id_source("adev").width(360.0).selected_text(sel_text).show_ui(ui, |ui| {
                    ui.selectable_value(&mut st.audio_device, String::new(), format!("Standardgerät ({std_name})"));
                    for d in self.audio_devices.iter() {
                        ui.selectable_value(&mut st.audio_device, d.clone(), d);
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Lautstärke");
                    ui.add(egui::Slider::new(&mut st.volume, 0.0..=1.5).custom_formatter(|v, _| format!("{} %", (v * 100.0).round() as i32)));
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Klang (wirkt sofort im Stream)").size(14.0).strong());
                for (lbl, v) in [("Bass", &mut st.eq_bass), ("Mitten", &mut st.eq_mid), ("Höhen", &mut st.eq_treble)] {
                    ui.horizontal(|ui| {
                        ui.add_sized([60.0, 20.0], egui::Label::new(RichText::new(lbl).size(13.0)));
                        ui.add(egui::Slider::new(v, -8.0..=8.0).custom_formatter(|x, _| format!("{:+.0} dB", x)));
                    });
                }
                if (st.eq_bass != 0.0 || st.eq_mid != 0.0 || st.eq_treble != 0.0) && ui.add(egui::Button::new("Klang zurücksetzen").fill(th_card_hover()).rounding(10.0)).clicked() {
                    st.eq_bass = 0.0;
                    st.eq_mid = 0.0;
                    st.eq_treble = 0.0;
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(format!("{} Ton testen", ic::SPEAKER_HIGH)).fill(th_card_hover()).rounding(10.0)).clicked() {
                        match audio::start_with(tone_pull(), &st.audio_device, 0.6 * st.volume.max(0.2)) {
                            Ok(a) => {
                                self.audio_test = Some((a, ui.ctx().input(|i| i.time)));
                                self.toasts.push(("Du solltest jetzt kurz einen leisen Ton hören.".to_string(), 0, self.now));
                            }
                            Err(e) => self.toasts.push((format!("Ton nicht möglich: {e}"), 1, self.now)),
                        }
                    }
                    if ui.add(egui::Button::new("Geräte neu laden").fill(th_card_hover()).rounding(10.0)).clicked() {
                        self.audio_devices = audio::list_devices();
                    }
                });
            });
            section(ui, &format!("{}  Aufnahmen", ic::RECORD), |ui| {
                ui.label(RichText::new("Im Stream: F9 startet und beendet eine Video-Aufnahme (MP4, Bild unverändert vom Stream, Ton als AAC). F12 macht einen Screenshot.").color(TEXT_DIM).size(12.5));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(format!("{} Videos öffnen", ic::FOLDER_OPEN)).fill(th_card_hover()).rounding(10.0)).clicked() {
                        if let Ok(base) = std::env::var("USERPROFILE") {
                            let d = std::path::PathBuf::from(base).join("Videos").join("OVERLIME");
                            let _ = std::fs::create_dir_all(&d);
                            let _ = std::process::Command::new("explorer").arg(d).spawn();
                        }
                    }
                    if ui.add(egui::Button::new(format!("{} Screenshots öffnen", ic::FOLDER_OPEN)).fill(th_card_hover()).rounding(10.0)).clicked() {
                        if let Ok(base) = std::env::var("USERPROFILE") {
                            let d = std::path::PathBuf::from(base).join("Pictures").join("OVERLIME");
                            let _ = std::fs::create_dir_all(&d);
                            let _ = std::process::Command::new("explorer").arg(d).spawn();
                        }
                    }
                });
            });
            section(ui, &format!("{}  Steuerung", ic::GAME_CONTROLLER), |ui| {
                ui.checkbox(&mut st.rumble, "Controller-Vibration");
                ui.checkbox(&mut st.ds_features, "DualSense-Funktionen: Touchpad, Bewegungssensor, adaptive Trigger, Lichtleiste (gilt ab dem nächsten Stream)");
                ui.label(RichText::new("Controller (PS5, PS4, Xbox, andere) werden automatisch erkannt. Die Tastatur stellst du weiter unten ein.").color(TEXT_DIM).size(12.5));
            });
            section(ui, &format!("{}  Tastatur", ic::KEYBOARD), |ui| {
                ui.label(RichText::new("Die Tastatur steuert das Spiel nur, wenn du sie brauchst. Die Kurztasten (Esc, F1, F10, F11, F12, M) funktionieren immer.").color(TEXT_DIM).size(12.5));
                ui.add_space(4.0);
                segmented(ui, &mut st.kb_mode, &["Automatisch (nur ohne Controller)", "Immer", "Aus"]);
                    egui::CollapsingHeader::new(RichText::new("Tasten anpassen").size(14.0).strong()).default_open(false).show(ui, |ui| {
                ui.label(RichText::new("Auf eine Taste klicken, dann die neue Taste drücken (Esc bricht ab).").color(TEXT_DIM).size(12.5));
                ui.add_space(4.0);
                egui::Grid::new("kbgrid").num_columns(2).spacing([20.0, 6.0]).show(ui, |ui| {
                    for (i, act) in keys::ACTIONS.iter().enumerate() {
                        ui.label(act.label);
                        let cap = self.capturing == Some(i);
                        let text = if cap { "Drücke eine Taste …".to_string() } else { keys::pretty(&st.keys[i]) };
                        let b = egui::Button::new(RichText::new(text).monospace())
                            .fill(if cap { th_blue() } else { th_card_hover() })
                            .rounding(8.0)
                            .min_size(Vec2::new(180.0, 28.0));
                        if ui.add(b).clicked() {
                            self.capturing = Some(i);
                            self.capture_since = ui.ctx().input(|inp| inp.time);
                            self.key_note.clear();
                        }
                        ui.end_row();
                    }
                });
                if !self.key_note.is_empty() {
                    ui.label(RichText::new(&self.key_note).color(YELLOW).size(12.5));
                }
                if ui.add(egui::Button::new("Auf Standard zurücksetzen").fill(th_card_hover()).rounding(10.0)).clicked() {
                    st.keys = keys::defaults();
                    self.capturing = None;
                    self.key_note.clear();
                }
                });
            });
            section(ui, &format!("{}  Discord", ic::DISCORD_LOGO), |ui| {
                let ready = !discord::DEFAULT_APP_ID.is_empty() || !st.discord_id.trim().is_empty();
                ui.add_enabled_ui(ready, |ui| {
                    ui.checkbox(&mut st.discord, "Meinen Status auf Discord anzeigen");
                    ui.checkbox(&mut st.discord_game, "Spielnamen anzeigen");
                });
                ui.label(RichText::new("Auf Discord steht „Spielt OVERLIME“ mit dem Spiel und der Konsole (PS5 oder PS4). Sonst wird nichts gesendet, keine IP-Adressen und keine Konsolennamen. Die Discord-App muss auf diesem PC laufen.").color(TEXT_DIM).size(12.5));
                if !ready {
                    ui.label(RichText::new("Noch nicht verfügbar: OVERLIME ist bei Discord noch nicht registriert.").color(YELLOW).size(12.5));
                } else if st.discord {
                    let (txt, col) = if self.discord.state() == discord::ST_CONNECTED {
                        ("Mit Discord verbunden.", GREEN)
                    } else {
                        ("Discord nicht gefunden – läuft die Discord-App?", YELLOW)
                    };
                    ui.label(RichText::new(txt).color(col).size(12.5));
                }

                // Live-Vorschau: so sehen es andere
                ui.add_space(8.0);
                ui.label(RichText::new("Vorschau: So sieht dein Status beim Spielen aus").size(13.0).strong());
                ui.add_space(2.0);
                if self.icon_tex.is_none() {
                    let ic_data = winui::app_icon();
                    let img = egui::ColorImage::from_rgba_unmultiplied([ic_data.width as usize, ic_data.height as usize], &ic_data.rgba);
                    self.icon_tex = Some(ui.ctx().load_texture("appicon", img, Default::default()));
                }
                let found = self.consoles.len();
                let (ex_game, ex_ps5) = match self.consoles.iter().find(|c| !c.running_app.is_empty()) {
                    Some(c) => (c.running_app.clone(), c.is_ps5),
                    None => ("EA SPORTS FC 26".to_string(), true),
                };
                let (det, stt) = presence_lines(&*st, 2, &ex_game, ex_ps5, found);
                let show_buttons = false;
                let tnow = ui.ctx().input(|i| i.time);
                let secs = (tnow as u64) % 3600;
                let h = if show_buttons { 176.0 } else { 128.0 };
                let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(460.0), h), egui::Sense::hover());
                let p = ui.painter_at(rect);
                p.rect_filled(rect, Rounding::same(14.0), Color32::from_rgb(30, 31, 34));
                p.text(rect.min + Vec2::new(16.0, 12.0), Align2::LEFT_TOP, "Spielt", FontId::proportional(12.5), Color32::from_rgb(220, 222, 225));
                let img_rect = Rect::from_min_size(rect.min + Vec2::new(16.0, 36.0), Vec2::splat(76.0));
                if let Some(tex) = &self.icon_tex {
                    p.image(tex.id(), img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
                let tx = rect.min.x + 108.0;
                p.text(Pos2::new(tx, rect.min.y + 36.0), Align2::LEFT_TOP, "OVERLIME", FontId::proportional(15.0), Color32::WHITE);
                p.text(Pos2::new(tx, rect.min.y + 58.0), Align2::LEFT_TOP, det, FontId::proportional(13.0), Color32::from_rgb(220, 222, 225));
                p.text(Pos2::new(tx, rect.min.y + 76.0), Align2::LEFT_TOP, stt, FontId::proportional(13.0), Color32::from_rgb(190, 193, 198));
                p.text(Pos2::new(tx, rect.min.y + 96.0), Align2::LEFT_TOP, format!("{}  {}:{:02}", ic::GAME_CONTROLLER, secs / 60, secs % 60), FontId::proportional(12.5), GREEN);
                if show_buttons {
                    let bw = (rect.width() - 48.0) / 2.0;
                    for (i, label) in ["OVERLIME herunterladen", "Quellcode ansehen"].iter().enumerate() {
                        let br = Rect::from_min_size(Pos2::new(rect.min.x + 16.0 + i as f32 * (bw + 16.0), rect.max.y - 44.0), Vec2::new(bw, 30.0));
                        p.rect_stroke(br, Rounding::same(6.0), Stroke::new(1.0, Color32::from_rgb(100, 103, 110)));
                        p.text(br.center(), Align2::CENTER_CENTER, *label, FontId::proportional(12.5), Color32::WHITE);
                    }
                }
                ui.ctx().request_repaint_after(Duration::from_millis(500));
            });
            section(ui, &format!("{}  Verbindung", ic::GLOBE), |ui| {
                label(ui, "Konsole per IP hinzufügen", "Nur für Internet/VPN (z. B. Tailscale). Im Heimnetz wird alles automatisch gefunden.");
                ui.add(egui::TextEdit::singleline(&mut st.manual_ip).hint_text("IP-Adresse der Konsole").desired_width(260.0));
                        if !st.psn_account.is_empty() && ui.button("PlayStation-Konto abmelden").clicked() {
                    st.psn_account.clear();
                    st.psn_name.clear();
                }
            });
            if SET_DRAWN.with(|d| d.get()) == 0 {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Nichts gefunden").size(16.0).strong());
                    ui.label(RichText::new("Versuch es mit einem anderen Begriff, z. B. „Bitrate“ oder „Ton“.").color(TEXT_DIM));
                });
            }
            ui.add_space(4.0);
            ui.label(RichText::new(format!("OVERLIME Remote Play {VERSION}")).color(TEXT_DIM).size(12.0));
        });
        SET_FILTER.with(|f| *f.borrow_mut() = None);
    }

    fn dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dialog.take() else {
            modal_reset(ctx, "pair");
            return;
        };
        let a = modal_progress(ctx, "pair");
        // Hintergrund abdunkeln
        egui::Area::new(egui::Id::new("dim")).order(egui::Order::Foreground).fixed_pos(Pos2::ZERO).show(ctx, |ui| {
            let r = ctx.screen_rect();
            ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_black_alpha((170.0 * a) as u8));
            ui.allocate_rect(r, egui::Sense::click());
        });
        let mut close = false;
        let mut next: Option<Step> = None;
        egui::Window::new("pair")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, (1.0 - a) * 18.0])
            .fixed_size([520.0, 0.0])
            .order(egui::Order::Tooltip)
            .frame(egui::Frame::none().fill(th_card().gamma_multiply(a)).rounding(20.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.5 * a))).inner_margin(egui::Margin::same(26.0)))
            .show(ctx, |ui| {
                ui.set_opacity(a);
                ui.set_width(468.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} koppeln", d.console.name)).size(22.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if d.step != Step::Busy && ui.add(egui::Button::new("X").fill(th_card_hover()).rounding(8.0)).clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(6.0);
                match d.step {
                    Step::Psn => {
                        ui.label(RichText::new("Einmalig: Melde dich mit deinem PlayStation-Konto an, damit die Konsole dich erkennt. Dein Passwort gibst du nur auf der Seite von Sony ein, die App sieht es nie.").color(TEXT_DIM));
                        ui.add_space(8.0);
                        step_row(ui, "1", "Klicke auf „Bei PlayStation anmelden“ und melde dich im Browser an.");
                        step_row(ui, "2", "Danach zeigt die Seite „redirect“ oder leer an. Kopiere die komplette Adresse aus der Adressleiste.");
                        step_row(ui, "3", "Füge sie hier ein und klicke auf „Weiter“.");
                        ui.add_space(6.0);
                        if ui.add(big_button("Bei PlayStation anmelden", th_card_hover()).min_size(Vec2::new(468.0, 44.0))).clicked() {
                            ctx.open_url(egui::OpenUrl::new_tab(psn::login_url()));
                        }
                        ui.add(egui::TextEdit::singleline(&mut d.pasted).hint_text("Adresse hier einfügen (…redirect?code=…)").desired_width(468.0));
                        if !d.message.is_empty() {
                            ui.label(RichText::new(&d.message).color(RED));
                        }
                        let code = psn::parse_code(&d.pasted);
                        if ui.add_enabled(code.is_some(), big_button("Weiter", th_blue()).min_size(Vec2::new(468.0, 46.0))).clicked() {
                            if let Some(code) = code {
                                let tx = self.tx.clone();
                                std::thread::spawn(move || {
                                    let m = match psn::account_id_from_code(&code) {
                                        Ok((id, name)) => Msg::PsnOk(id, name),
                                        Err(e) => Msg::PsnErr(e),
                                    };
                                    let _ = tx.send(m);
                                });
                                next = Some(Step::Busy);
                            }
                        }
                    }
                    Step::Pin => {
                        ui.label(RichText::new("Jetzt verbindest du die App mit deiner Konsole.").color(TEXT_DIM));
                        ui.add_space(8.0);
                        step_row(ui, "1", "Schalte die Konsole ein (nicht Ruhemodus).");
                        let where_ = if d.console.is_ps5 {
                            "Öffne an der PS5: Einstellungen > System > Remote Play > „Gerät koppeln“."
                        } else {
                            "Öffne an der PS4: Einstellungen > Remote-Play-Verbindungseinstellungen > „Gerät hinzufügen“."
                        };
                        step_row(ui, "2", where_);
                        step_row(ui, "3", "Die Konsole zeigt einen 8-stelligen Code. Gib ihn hier ein:");
                        ui.add_space(4.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut d.pin)
                                .hint_text("12345678")
                                .font(FontId::monospace(28.0))
                                .desired_width(468.0)
                                .horizontal_align(Align::Center),
                        );
                        d.pin.retain(|c| c.is_ascii_digit());
                        d.pin.truncate(8);
                        let ok = d.pin.len() == 8;
                        if ui.add_enabled(ok, big_button("Koppeln", th_blue()).min_size(Vec2::new(468.0, 46.0))).clicked() {
                            let tx = self.tx.clone();
                            let acct = psn::id_from_b64(&self.settings.psn_account);
                            let (host, ver, ps5, id) = (d.console.addr.clone(), d.console.system_version.clone(), d.console.is_ps5, d.console.id.clone());
                            let pin: u32 = d.pin.parse().unwrap_or(0);
                            std::thread::spawn(move || {
                                let m = match acct {
                                    Some(a) => match ffi::register(&host, &ver, ps5, a, pin, &id) {
                                        Ok(p) => Msg::PairOk(p),
                                        Err(e) => Msg::PairErr(e),
                                    },
                                    None => Msg::PairErr("PlayStation-Konto fehlt".into()),
                                };
                                let _ = tx.send(m);
                            });
                            next = Some(Step::Busy);
                        }
                    }
                    Step::Busy => {
                        ui.add_space(20.0);
                        ui.vertical_centered(|ui| {
                            ui.add(egui::Spinner::new().size(40.0));
                            ui.add_space(10.0);
                            ui.label(RichText::new("Bitte warten …").size(16.0));
                        });
                        ui.add_space(20.0);
                    }
                    Step::Done => {
                        ui.add_space(10.0);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("OK").size(40.0).strong().color(GREEN));
                            ui.label(RichText::new("Erfolgreich gekoppelt!").size(20.0).strong());
                            ui.label(RichText::new("Du kannst jetzt „Verbinden“ drücken.").color(TEXT_DIM));
                        });
                        ui.add_space(10.0);
                        if ui.add(big_button("Fertig", th_blue()).min_size(Vec2::new(468.0, 46.0))).clicked() {
                            close = true;
                        }
                    }
                    Step::Failed => {
                        ui.label(RichText::new("Kopplung fehlgeschlagen").size(18.0).strong().color(RED));
                        ui.label(RichText::new(&d.message).color(TEXT_DIM));
                        ui.add_space(4.0);
                        ui.label(RichText::new("Tipp: Der Code gilt nur kurz. Öffne „Gerät koppeln“ an der Konsole neu und gib den neuen Code ein.").color(TEXT_DIM).size(13.0));
                        ui.add_space(8.0);
                        if ui.add(big_button("Nochmal versuchen", th_blue()).min_size(Vec2::new(468.0, 46.0))).clicked() {
                            d.pin.clear();
                            next = Some(Step::Pin);
                        }
                    }
                }
            });
        if let Some(s) = next {
            d.step = s;
            d.message.clear();
        }
        if !close {
            self.dialog = Some(d);
        }
    }

    /// Schnell-Einstellungen im Stream. Gibt true zurück, wenn die Qualität neu angewendet werden soll.
    fn quick_panel(&mut self, ctx: &egui::Context) -> bool {
        if !self.quick {
            modal_reset(ctx, "quick");
            return false;
        }
        let a = modal_progress(ctx, "quick");
        let mut apply = false;
        let running = self.last_connect.as_ref().map(|(_, _, pr)| *pr);
        let cur = (self.settings.resolution, self.settings.fps, self.settings.bitrate as i32, self.settings.codec);
        let differs = running.map(|r| r != cur).unwrap_or(false);
        let demo = std::env::args().any(|a| a == "--demo-quick");
        let (hist, ping_now, fps_now, mbit_now, loss_now): (Vec<(f32, f32, f32)>, f32, u32, f32, f32) = match self.session.as_ref() {
            Some(s) => (s.hist.clone(), s.ping, s.fps, s.kbps as f32 / 1000.0, s.loss_rate),
            None if demo => ((0..90).map(|i| (9.0 + 4.0 * ((i as f32) * 0.35).sin().abs() + if i % 23 == 0 { 14.0 } else { 0.0 }, 27.0 + 2.0 * ((i as f32) * 0.2).cos(), if i % 31 == 0 { 3.0 } else { 0.0 })).collect(), 11.0, 60, 27.4, 0.0),
            None => (Vec::new(), 0.0, 0, 0.0, 0.0),
        };
        egui::Window::new("quick")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::RIGHT_TOP, [-14.0 + (1.0 - a) * 40.0, 64.0])
            .order(egui::Order::Foreground)
            .frame(egui::Frame::none().fill(Color32::from_rgba_unmultiplied(12, 16, 32, (244.0 * a) as u8)).rounding(18.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.45 * a))).inner_margin(egui::Margin::same(16.0)))
            .show(ctx, |ui| {
                ui.set_opacity(a);
                ui.set_width(380.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}  Schnell-Einstellungen", ic::SLIDERS)).size(17.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(egui::Button::new("X").fill(th_card_hover()).rounding(8.0)).clicked() {
                            self.quick = false;
                        }
                    });
                });
                ui.add_space(6.0);
                let max_h = (ctx.screen_rect().height() - 100.0).max(240.0);
                egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
                // Live-Werte
                ui.horizontal_wrapped(|ui| {
                    let pc = if ping_now < 25.0 { GREEN } else if ping_now < 60.0 { YELLOW } else { RED };
                    stat_chip(ui, &format!("Ping {ping_now:.0} ms"), pc);
                    stat_chip(ui, &format!("{fps_now} FPS"), if fps_now >= 50 { GREEN } else if fps_now >= 30 { YELLOW } else { RED });
                    stat_chip(ui, &format!("{mbit_now:.0} Mbit/s"), th_blue_light());
                    stat_chip(ui, &format!("Verlust {loss_now:.0}/s"), if loss_now < 1.0 { GREEN } else if loss_now < 5.0 { YELLOW } else { RED });
                });
                ui.add_space(6.0);
                let pings: Vec<f32> = hist.iter().map(|h| h.0).collect();
                sparkline(ui, &pings, th_blue_light(), 20.0, "ms Ping");
                ui.add_space(2.0);
                let rates: Vec<f32> = hist.iter().map(|h| h.1).collect();
                sparkline(ui, &rates, GREEN, 10.0, "Mbit/s");
                ui.add_space(8.0);
                card(ui, &format!("{}  Bild (wirkt sofort)", ic::MONITOR), |ui| {
                    segmented(ui, &mut self.settings.upscale, &["Aus", "Weich", "Scharf", "Ultra"]);
                    ui.add_enabled_ui(self.settings.upscale >= 2, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Stärke").size(12.5).color(TEXT_DIM));
                            ui.add(egui::Slider::new(&mut self.settings.sharp, 0.0..=1.0).show_value(false));
                        });
                    });
                    ui.checkbox(&mut self.show_hud, "Statusanzeige im Stream (F10)");
                    if self.show_hud {
                        ui.add_space(2.0);
                        hud_config_ui(ui, &mut self.settings);
                    }
                });
                ui.add_space(8.0);
                card(ui, &format!("{}  Qualität", ic::FILM_STRIP), |ui| {
                    // Voreinstellungen mit einem Klick
                    let presets: [(&str, (usize, usize, f32, usize)); 4] = [
                        ("Beste", (2, 1, 50.0, 2)),
                        ("Ausgewogen", (2, 1, 30.0, 0)),
                        ("Sparsam", (1, 1, 15.0, 0)),
                        ("Schwaches Netz", (1, 0, 8.0, 1)),
                    ];
                    ui.horizontal_wrapped(|ui| {
                        for (name, (r, f, b, c)) in presets {
                            let on = self.settings.resolution == r && self.settings.fps == f && (self.settings.bitrate - b).abs() < 0.5 && self.settings.codec == c;
                            let btn = egui::Button::new(RichText::new(name).size(12.5).color(if on { Color32::WHITE } else { TEXT_DIM }))
                                .fill(if on { th_blue() } else { th_card_hover() })
                                .rounding(12.0);
                            if ui.add(btn).clicked() {
                                self.settings.resolution = r;
                                self.settings.fps = f;
                                self.settings.bitrate = b;
                                self.settings.codec = c;
                            }
                        }
                    });
                    ui.add_space(4.0);
                    segmented(ui, &mut self.settings.resolution, &["540p", "720p", "1080p"]);
                    segmented(ui, &mut self.settings.fps, &["30 FPS", "60 FPS"]);
                    ui.add_space(4.0);
                    fancy_switch(ui, &mut self.settings.interp, "Zwischenbilder für 120 Hz", "", "Experimentell");
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Bitrate").size(12.5).color(TEXT_DIM));
                        ui.spacing_mut().slider_width = 190.0;
                        ui.add(egui::Slider::new(&mut self.settings.bitrate, 2.0..=50.0).suffix(" Mbit/s"));
                    });
                    segmented(ui, &mut self.settings.codec, &["Auto", "H.264", "HEVC"]);
                    ui.add_space(6.0);
                    if differs {
                        ui.label(RichText::new(format!("Neu: {} • {} FPS • {} Mbit/s", ["540p", "720p", "1080p"][self.settings.resolution], [30, 60][self.settings.fps], self.settings.bitrate as i32)).size(12.5).color(th_blue_light()));
                        if ui.add(big_button(&format!("{}  Anwenden und neu verbinden", ic::ARROWS_CLOCKWISE), th_blue()).min_size(Vec2::new(ui.available_width(), 42.0))).clicked() {
                            apply = true;
                        }
                        ui.label(RichText::new("Das dauert etwa 5 Sekunden. Dein Spiel läuft auf der Konsole weiter.").color(TEXT_DIM).size(11.5));
                    } else {
                        ui.label(RichText::new(format!("{}  Diese Qualität läuft gerade.", ic::CHECK_CIRCLE)).color(GREEN).size(12.5));
                    }
                });
                });
            });
        apply
    }

    /// Pause-Menü und Tastenbelegung im Stream. Gibt (weiter, vollbild, tasten, stream_beenden, app_beenden) zurück.
    fn stream_overlays(&mut self, ctx: &egui::Context) -> (bool, bool, bool, bool, bool) {
        let (mut resume, mut fs, mut keys_btn, mut stop, mut quit) = (false, false, false, false, false);
        let mut shot = false;
        if !self.stream_menu {
            modal_reset(ctx, "pause");
        }
        if self.stream_menu {
            let a = modal_progress(ctx, "pause");
            egui::Area::new(egui::Id::new("menu_dim")).order(egui::Order::Foreground).fixed_pos(Pos2::ZERO).show(ctx, |ui| {
                let r = ctx.screen_rect();
                ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_black_alpha((150.0 * a) as u8));
                ui.allocate_rect(r, egui::Sense::click());
            });
            egui::Window::new("pause")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, (1.0 - a) * 18.0])
                .order(egui::Order::Tooltip)
                .frame(egui::Frame::none().fill(th_card().gamma_multiply(a)).rounding(20.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.5 * a))).inner_margin(egui::Margin::same(26.0)))
                .show(ctx, |ui| {
                    ui.set_opacity(a);
                    ui.set_width(360.0);
                    ui.label(RichText::new("Pause-Menü").size(22.0).strong());
                    if self.close_hint {
                        ui.label(RichText::new("Der Stream läuft noch. Willst du wirklich schließen?").color(YELLOW).size(13.5));
                    } else {
                        ui.label(RichText::new("Dein Spiel läuft auf der Konsole weiter.").color(TEXT_DIM).size(13.0));
                    }
                    ui.add_space(8.0);
                    if ui.add(big_button("Weiter (Esc)", th_blue()).min_size(Vec2::new(360.0, 46.0))).clicked() {
                        resume = true;
                    }
                    ui.add_space(6.0);
                    let tw = (360.0 - 2.0 * 10.0) / 3.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        if tile(ui, ic::CORNERS_OUT, "Vollbild", tw, false) {
                            fs = true;
                        }
                        if tile(ui, ic::KEYBOARD, "Tasten", tw, false) {
                            keys_btn = true;
                        }
                        if tile(ui, ic::CAMERA, "Screenshot", tw, false) {
                            shot = true;
                        }
                    });
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let rec_on = self.rec_started.is_some();
                        if tile(ui, ic::RECORD, if rec_on { "Aufnahme aus" } else { "Aufnehmen" }, tw, rec_on) {
                            self.rec_toggle = true;
                            self.stream_menu = false;
                        }
                        if tile(ui, ic::SLIDERS, "Qualität", tw, false) {
                            self.quick = true;
                            self.stream_menu = false;
                        }
                        if tile(ui, if self.muted { ic::SPEAKER_X } else { ic::SPEAKER_HIGH }, if self.muted { "Ton an" } else { "Ton aus" }, tw, self.muted) {
                            self.muted = !self.muted;
                        }
                    });
                    ui.add_space(2.0);
                    ui.label(RichText::new("F11 Vollbild  •  F12 Screenshot  •  F9 Aufnahme  •  M Ton").color(TEXT_DIM).size(11.0));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}  Lautstärke", ic::SPEAKER_HIGH)).size(14.0));
                        ui.spacing_mut().slider_width = 120.0;
                        ui.add(egui::Slider::new(&mut self.settings.volume, 0.0..=1.5).show_value(false));
                        ui.checkbox(&mut self.muted, "Stumm");
                    });
                    ui.add_space(4.0);
                    if ui.add(big_button("Stream beenden", RED.gamma_multiply(0.55)).min_size(Vec2::new(360.0, 42.0))).clicked() {
                        stop = true;
                    }
                    if ui.add(egui::Button::new(RichText::new("OVERLIME komplett schließen").color(TEXT_DIM)).frame(false)).clicked() {
                        quit = true;
                    }
                });
        }
        if self.show_keys {
            egui::Window::new("keys")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::LEFT_TOP, [16.0, 40.0])
                .order(egui::Order::Foreground)
                .frame(egui::Frame::none().fill(Color32::from_black_alpha(215)).rounding(14.0).inner_margin(egui::Margin::same(16.0)))
                .show(ctx, |ui| {
                    ui.label(RichText::new("Tastenbelegung (F1 oder Esc schließt)").strong());
                    ui.add_space(4.0);
                    egui::Grid::new("keys_ov").num_columns(4).spacing([18.0, 3.0]).show(ui, |ui| {
                        for (n, act) in keys::ACTIONS.iter().enumerate() {
                            ui.label(RichText::new(keys::pretty(&self.settings.keys[n])).monospace().color(th_blue_light()).size(12.0));
                            ui.label(RichText::new(act.label).size(12.0));
                            if n % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new("Esc = Menü   F11 = Vollbild   F10 = Statusanzeige").color(TEXT_DIM).size(11.5));
                });
        }
        if shot {
            self.shot_requested = true;
            self.stream_menu = false;
        }
        (resume, fs, keys_btn, stop, quit)
    }

    /// Tasten-Belegung aufnehmen (Einstellungen, Abschnitt Tastatur).
    fn process_capture(&mut self, ctx: &egui::Context, t: f64) {
        let Some(idx) = self.capturing else { return };
        if t - self.capture_since < 0.2 {
            return;
        }
        let mut chosen: Option<String> = None;
        let mut cancel = false;
        ctx.input(|i| {
            for e in &i.events {
                if let egui::Event::Key { key, pressed: true, .. } = e {
                    if *key == Key::Escape {
                        cancel = true;
                    } else {
                        chosen = Some(key.name().to_string());
                    }
                }
            }
            if chosen.is_none() && !cancel {
                if i.modifiers.shift {
                    chosen = Some("Shift".into());
                } else if i.modifiers.ctrl {
                    chosen = Some("Ctrl".into());
                } else if i.modifiers.alt {
                    chosen = Some("Alt".into());
                }
            }
        });
        if cancel {
            self.capturing = None;
            return;
        }
        if let Some(name) = chosen {
            let mut note = String::new();
            for (j, k) in self.settings.keys.iter_mut().enumerate() {
                if j != idx && *k == name {
                    *k = String::new();
                    note = format!("{} war schon bei \"{}\" belegt und wurde dort entfernt.", keys::pretty(&name), keys::ACTIONS[j].label);
                }
            }
            self.settings.keys[idx] = name;
            self.key_note = note;
            self.capturing = None;
        }
    }

    /// Discord-Status: nur wenn aktiviert. Zeigt keine IP, keinen Konsolennamen.
    fn update_discord(&mut self) {
        let want = if !self.settings.discord {
            None
        } else {
            let id = if self.settings.discord_id.trim().is_empty() { discord::DEFAULT_APP_ID.to_string() } else { self.settings.discord_id.trim().to_string() };
            let large = format!("OVERLIME Remote Play {VERSION}");
            let found = self.consoles.len();
            let (mode, game, ps5, start) = match self.session.as_ref() {
                Some(s) if s.state == 1 => {
                    let con = self.consoles.iter().find(|c| c.name == s.name);
                    (2u8, con.map(|c| c.running_app.clone()).unwrap_or_default(), con.map(|c| c.is_ps5).unwrap_or(true), Some(s.started_unix))
                }
                Some(_) => (1u8, String::new(), true, None),
                None => (0u8, String::new(), true, Some(self.app_start_unix)),
            };
            let (details, state) = presence_lines(&self.settings, mode, &game, ps5, found);
            Some(discord::Presence { app_id: id, details, state, start, buttons: false, large_text: large })
        };
        self.discord.update(want);
    }

    /// Einblend-Benachrichtigungen unten rechts.
    fn draw_toasts(&mut self, ctx: &egui::Context, t: f64) {
        self.toasts.retain(|(_, _, t0)| t - *t0 < 5.0);
        let n = self.toasts.len();
        for (i, (text, kind, t0)) in self.toasts.iter().enumerate() {
            let age = (t - *t0) as f32;
            let appear = ease_out_t(age / 0.35);
            let fade_out = (1.0 - ((age - 4.4) / 0.6).clamp(0.0, 1.0)).max(0.0);
            let col = match kind { 1 => RED, 2 => GREEN, _ => th_blue_light() };
            let slot = (n - 1 - i) as f32;
            egui::Area::new(egui::Id::new(("toast", i)))
                .order(egui::Order::Tooltip)
                .anchor(Align2::RIGHT_BOTTOM, [-18.0 + (1.0 - appear) * 60.0, -18.0 - slot * 64.0])
                .show(ctx, |ui| {
                    ui.set_opacity(appear * fade_out);
                    egui::Frame::none()
                        .fill(th_card())
                        .rounding(14.0)
                        .stroke(Stroke::new(1.0, col.gamma_multiply(0.55)))
                        .inner_margin(egui::Margin { left: 18.0, right: 18.0, top: 12.0, bottom: 12.0 })
                        .show(ui, |ui| {
                            ui.set_max_width(340.0);
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(Vec2::new(5.0, 22.0), egui::Sense::hover());
                                ui.painter().rect_filled(r, Rounding::same(3.0), col);
                                ui.add(egui::Label::new(RichText::new(text).size(13.5)).selectable(false));
                            });
                        });
                });
        }
        if !self.toasts.is_empty() {
            ctx.request_repaint();
        }
    }

    /// Nach jeder Suche: "bereit"-Hinweis und automatisches Verbinden beim Start.
    fn after_scan(&mut self) {
        let list = self.consoles.clone();
        for c in &list {
            if self.paired_for(c).is_some() {
                let ready = c.state == State::Ready;
                let prev = self.prev_ready.insert(c.id.clone(), ready);
                if prev == Some(false) && ready {
                    self.toasts.push((format!("{} ist jetzt bereit", c.name), 2, self.now));
                }
            }
        }
        if (self.settings.autoconnect || self.connect_now) && !self.auto_done && self.session.is_none() && self.starting.is_none() {
            if let Some(c) = list.iter().find(|c| c.state == State::Ready && self.paired_for(c).is_some()) {
                if let Some(p) = self.paired_for(c) {
                    self.auto_done = true;
                    self.connect_now = false;
                    self.toasts.push((format!("Verbinde automatisch mit {} …", c.name), 0, self.now));
                    self.connect(c, &p);
                }
            } else if !self.auto_wake_done {
                if let Some(c) = list.iter().find(|c| c.state == State::Standby && self.paired_for(c).is_some()) {
                    if let Some(p) = self.paired_for(c) {
                        self.auto_wake_done = true;
                        self.wake(c, &p);
                    }
                }
            }
        }
    }

    /// Bericht für Fehlersuche (ohne IP-Adressen, ohne Passwörter).
    fn diag_report(&self) -> String {
        let mut r = format!("OVERLIME {VERSION}\n");
        r.push_str(&format!("Qualität: {} • {} FPS • {} Mbit/s • Codec {}\n", ["540p", "720p", "1080p"][self.settings.resolution], [30, 60][self.settings.fps], self.settings.bitrate as i32, ["Auto", "H.264", "HEVC"][self.settings.codec]));
        if !self.gl_info.is_empty() {
            r.push_str(&format!("Grafik: {}\n", self.gl_info));
        }
        r.push_str(&format!("Netzwerk: {}\n", if self.local_ip == "?" { "offline" } else { "verbunden" }));
        r.push_str(&format!("Angemeldet: {}\n", if psn::id_from_b64(&self.settings.psn_account).is_some() { "ja" } else { "nein" }));
        r.push_str(&format!("Controller: {}\n", if self.pad_name_cache.is_empty() { "keiner".to_string() } else { self.pad_name_cache.clone() }));
        for (name, usable, vid, pid) in self.pad.devices() {
            r.push_str(&format!("Gerät: {} ({:04x}:{:04x}) {}\n", name, vid, pid, if usable { "als Gamepad nutzbar" } else { "nicht als Gamepad erkannt" }));
        }
        r.push_str(&format!("Konsolen gefunden: {}\n", self.consoles.len()));
        for c in &self.consoles {
            r.push_str(&format!("- {} ({}, Software {}), gekoppelt: {}\n", if c.is_ps5 { "PS5" } else { "PS4" }, match c.state { State::Ready => "bereit", State::Standby => "Ruhemodus", State::Unknown => "unbekannt" }, pretty_version(&c.system_version), if self.paired_for(c).is_some() { "ja" } else { "nein" }));
        }
        if let Some((ok, t)) = &self.diag_port {
            r.push_str(&format!("Verbindungstest: {} - {}\n", if *ok { "ok" } else { "fehlgeschlagen" }, t));
        }
        if self.crash_pending {
            if let Some(c) = crash_path().and_then(|p| std::fs::read_to_string(p).ok()) {
                r.push_str("\n--- Absturzbericht ---\n");
                r.push_str(&c.chars().take(6000).collect::<String>());
            }
        }
        r.push_str("\n--- Protokoll (letzte Zeilen) ---\n");
        let tail: Vec<&str> = self.log.lines().rev().take(150).collect();
        for l in tail.into_iter().rev() {
            r.push_str(&mask_ips(l));
            r.push('\n');
        }
        r
    }

    /// Fenster "Problem melden": schickt Beschreibung, Bericht und Protokoll an den Entwickler.
    fn report_ui(&mut self, ctx: &egui::Context) {
        if !self.report_open {
            modal_reset(ctx, "report");
            return;
        }
        let a = modal_progress(ctx, "report");
        egui::Area::new(egui::Id::new("rp_dim")).order(egui::Order::Foreground).fixed_pos(Pos2::ZERO).show(ctx, |ui| {
            let r = ctx.screen_rect();
            ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_black_alpha((170.0 * a) as u8));
            ui.allocate_rect(r, egui::Sense::click());
        });
        let mut close = false;
        let mut send = false;
        egui::Window::new("report")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, (1.0 - a) * 18.0])
            .order(egui::Order::Tooltip)
            .frame(egui::Frame::none().fill(th_card().gamma_multiply(a)).rounding(20.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.5 * a))).inner_margin(egui::Margin::same(26.0)))
            .show(ctx, |ui| {
                ui.set_opacity(a);
                ui.set_width(480.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if self.crash_pending { "OVERLIME wurde unerwartet beendet" } else { "Problem melden" }).size(22.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.report_state != 1 && ui.add(egui::Button::new("X").fill(th_card_hover()).rounding(8.0)).clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(6.0);
                if self.report_state == 2 {
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(ic::CHECK_CIRCLE).size(46.0).color(GREEN));
                        ui.label(RichText::new("Danke, deine Meldung ist angekommen!").size(18.0).strong());
                        ui.label(RichText::new("Ich schaue sie mir an und melde mich, falls ich etwas wissen muss.").color(TEXT_DIM).size(13.0));
                    });
                    ui.add_space(12.0);
                    if ui.add(big_button("Schließen", th_blue()).min_size(Vec2::new(480.0, 44.0))).clicked() {
                        close = true;
                    }
                    return;
                }
                ui.label(RichText::new(if self.crash_pending { "Möchtest du den Fehlerbericht senden? Das hilft, den Fehler zu beheben. Ergänze gern, was du gerade gemacht hast." } else { "Beschreibe kurz, was du tun wolltest und was passiert ist." }).color(TEXT_DIM).size(13.5));
                ui.add_space(6.0);
                ui.add_enabled(self.report_state != 1, egui::TextEdit::multiline(&mut self.report_text).hint_text("z. B. „Beim Verbinden mit der PS5 bleibt das Bild schwarz“").desired_rows(5).desired_width(480.0));
                self.report_text.truncate(2000);
                ui.add_space(8.0);
                egui::Frame::none().fill(th_bg().gamma_multiply(0.6)).rounding(12.0).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                    ui.set_width(456.0);
                    ui.horizontal_top(|ui| {
                        ui.label(RichText::new(ic::LOCK_KEY).size(18.0).color(GREEN));
                        ui.add(egui::Label::new(RichText::new("Mitgeschickt werden: OVERLIME-Version, Qualitäts-Einstellungen, Controller, gefundene Konsolen, das Protokoll und bei einem Absturz dessen Fehlermeldung. IP-Adressen, Passwörter und deine Konto-Nummer sind nicht dabei.").color(TEXT_DIM).size(12.5)).wrap());
                    });
                });
                if self.report_state == 3 {
                    ui.add_space(6.0);
                    ui.label(RichText::new(&self.report_msg).color(RED).size(13.0));
                }
                ui.add_space(10.0);
                if self.report_state == 1 {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Sende …").color(TEXT_DIM));
                    });
                } else {
                    let ok = self.report_text.trim().chars().count() >= 5;
                    if ui.add_enabled(ok, big_button("Senden", th_blue()).min_size(Vec2::new(480.0, 46.0))).clicked() {
                        send = true;
                    }
                    if !ok {
                        ui.label(RichText::new("Bitte ein paar Worte schreiben.").color(TEXT_DIM).size(12.0));
                    }
                }
            });
        if close {
            if self.crash_pending {
                // nicht senden: die Absturzdatei wird gelöscht
                if let Some(p) = crash_path() {
                    let _ = std::fs::remove_file(p);
                }
                self.crash_pending = false;
            }
            self.report_open = false;
            if self.report_state == 2 {
                self.report_text.clear();
            }
            self.report_state = 0;
        }
        if send {
            if self.now - self.report_last < 30.0 && self.report_last > 0.0 {
                self.report_state = 3;
                self.report_msg = "Bitte warte kurz, bevor du noch eine Meldung schickst.".to_string();
            } else {
                self.report_last = self.now;
                self.report_state = 1;
                let report = self.diag_report();
                let text = self.report_text.trim().to_string();
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let body = serde_json::json!({ "version": VERSION, "message": text, "report": report });
                    let m = match ureq::post(RELAY_URL).timeout(Duration::from_secs(15)).set("X-Overlime", "1").send_json(body) {
                        Ok(_) => Msg::ReportDone(true, String::new()),
                        Err(e) => Msg::ReportDone(false, format!("Senden nicht möglich ({}). Prüfe die Internetverbindung.", mask_ips(&e.to_string()))),
                    };
                    let _ = tx.send(m);
                });
            }
        }
    }

    /// Kopiert den Bericht und öffnet die Meldeseite auf GitHub.
    fn report_problem(&mut self, ctx: &egui::Context, title: &str) {
        ctx.copy_text(self.diag_report());
        let url = format!(
            "https://github.com/Overtime09991/overlime/issues/new?title={}&body={}",
            psn::enc(title),
            psn::enc("Bitte hier den Bericht aus der Zwischenablage einfügen (Strg+V) und kurz beschreiben, was nicht klappt.\n\nWas habe ich getan?\n\nWas ist passiert?\n")
        );
        ctx.open_url(egui::OpenUrl::new_tab(url));
        self.toasts.push(("Bericht kopiert. Auf der GitHub-Seite mit Strg+V einfügen.".to_string(), 0, self.now));
    }

    /// Kurze Einführung mit weichen Übergängen. Wird einmal gezeigt und lässt sich unter Hilfe > Über wiederholen.
    fn tour_ui(&mut self, ctx: &egui::Context) {
        let Some(step) = self.tour else {
            modal_reset(ctx, "tour");
            return;
        };
        let a = modal_progress(ctx, "tour");
        let steps: [(&str, &str, &str); 5] = [
            (ic::ROCKET_LAUNCH, "Willkommen bei OVERLIME", "Eine kurze Tour zeigt dir in einer halben Minute, wie alles funktioniert. Du kannst sie jederzeit überspringen."),
            (ic::GAME_CONTROLLER, "Konsole verbinden", "Auf der Seite „Konsolen“ siehst du deine PS4 und PS5. Einmal koppelst du sie mit dem Code von der Konsole, danach genügt ein Klick auf „Verbinden“."),
            (ic::KEYBOARD, "Im Spiel", "Esc öffnet das Menü, F11 schaltet auf Vollbild, F12 macht einen Screenshot, F9 nimmt ein Video auf und M schaltet den Ton aus oder an."),
            (ic::POWER, "Symbol neben der Uhr", "In den Einstellungen kannst du das kleine Symbol einschalten. Ein Klick darauf weckt die Konsole und verbindet automatisch. Auf Wunsch auch zu einer festen Uhrzeit."),
            (ic::QUESTION, "Hilfe, wenn etwas klemmt", "Unter „Hilfe & Diagnose“ prüft OVERLIME alles für dich, hilft beim Controller und lässt dich Probleme direkt melden."),
        ];
        let last = steps.len() - 1;
        let (mut next, mut skip) = (false, false);
        egui::Area::new(egui::Id::new("tour_dim")).order(egui::Order::Foreground).fixed_pos(Pos2::ZERO).show(ctx, |ui| {
            let r = ctx.screen_rect();
            ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_black_alpha((170.0 * a) as u8));
            ui.allocate_rect(r, egui::Sense::click());
        });
        egui::Window::new("tour")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, (1.0 - a) * 18.0])
            .order(egui::Order::Tooltip)
            .frame(egui::Frame::none().fill(th_card().gamma_multiply(a)).rounding(22.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.5 * a))).inner_margin(egui::Margin::same(30.0)))
            .show(ctx, |ui| {
                ui.set_opacity(a);
                ui.set_width(440.0);
                let (icn, title, text) = steps[step];
                // Inhalt gleitet beim Weiterschalten sanft ein
                let k = ctx.animate_value_with_time(egui::Id::new("tour_step_anim"), step as f32, 0.35);
                let moving = (k - step as f32).abs();
                ui.vertical_centered(|ui| {
                    let (rc, _) = ui.allocate_exact_size(Vec2::splat(76.0), egui::Sense::hover());
                    ui.painter().circle_filled(rc.center(), 38.0, th_blue().gamma_multiply(0.25));
                    ui.painter().text(rc.center(), Align2::CENTER_CENTER, icn, FontId::proportional(36.0), th_blue_light());
                    ui.add_space(10.0);
                    ui.label(RichText::new(title).size(24.0).strong().color(Color32::WHITE.gamma_multiply(1.0 - moving.min(1.0) * 0.6)));
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(RichText::new(text).color(TEXT_DIM).size(14.5)).wrap());
                });
                ui.add_space(16.0);
                // Fortschrittspunkte
                ui.horizontal(|ui| {
                    let w = steps.len() as f32 * 22.0;
                    ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                    for i in 0..steps.len() {
                        let (rc, _) = ui.allocate_exact_size(Vec2::new(22.0, 10.0), egui::Sense::hover());
                        let on = ctx.animate_bool_with_time(egui::Id::new(("tour_dot", i)), i == step, 0.3);
                        let r = 3.0 + 2.0 * on;
                        ui.painter().circle_filled(rc.center(), r, TEXT_DIM.gamma_multiply(0.35).lerp_to_gamma(th_blue_light(), on));
                    }
                });
                ui.add_space(14.0);
                let label = if step == last { "Los geht's" } else { "Weiter" };
                if ui.add(big_button(label, th_blue()).min_size(Vec2::new(440.0, 48.0))).clicked() {
                    next = true;
                }
                if step < last {
                    ui.add_space(4.0);
                    ui.vertical_centered(|ui| {
                        if ui.add(egui::Button::new(RichText::new("Tour überspringen").color(TEXT_DIM)).frame(false)).clicked() {
                            skip = true;
                        }
                    });
                }
            });
        if next {
            if step >= last {
                skip = true;
            } else {
                self.tour = Some(step + 1);
            }
        }
        if skip {
            self.tour = None;
            self.settings.tour_done = true;
        }
    }

    /// Hält das Test-Ergebnis lokal fest (nur auf diesem PC, wird nirgends hingeschickt).
    fn save_feedback(&self, result: &str) {
        use std::io::Write;
        let Ok(base) = std::env::var("APPDATA") else { return };
        let dir = std::path::PathBuf::from(base).join("overlime");
        let _ = std::fs::create_dir_all(&dir);
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("feedback.txt")) {
            let _ = writeln!(f, "{secs} | Version {VERSION} | {result}");
        }
    }

    /// Kurze Abfrage nach dem Streamen: gut oder schlecht.
    fn feedback_ui(&mut self, ctx: &egui::Context) {
        let Some(err) = self.feedback else {
            modal_reset(ctx, "feedback");
            return;
        };
        let a = modal_progress(ctx, "feedback");
        egui::Area::new(egui::Id::new("fb_dim")).order(egui::Order::Foreground).fixed_pos(Pos2::ZERO).show(ctx, |ui| {
            let r = ctx.screen_rect();
            ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_black_alpha((150.0 * a) as u8));
            ui.allocate_rect(r, egui::Sense::click());
        });
        let (mut good, mut bad, mut later) = (false, false, false);
        egui::Window::new("feedback")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, (1.0 - a) * 18.0])
            .order(egui::Order::Tooltip)
            .frame(egui::Frame::none().fill(th_card().gamma_multiply(a)).rounding(20.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.5 * a))).inner_margin(egui::Margin::same(26.0)))
            .show(ctx, |ui| {
                ui.set_opacity(a);
                ui.set_width(400.0);
                if err {
                    ui.label(RichText::new("Die Verbindung hat nicht geklappt").size(21.0).strong().color(RED));
                    ui.label(RichText::new("Mit einem Klick schickst du den Fehlerbericht ab, damit wir den Grund finden. Er enthält keine IP-Adressen und keine Passwörter.").color(TEXT_DIM).size(13.0));
                    ui.add_space(10.0);
                    if ui.add(big_button("Problem melden", th_blue()).min_size(Vec2::new(400.0, 46.0))).clicked() {
                        bad = true;
                    }
                } else {
                    ui.label(RichText::new("Wie lief das Streamen?").size(22.0).strong());
                    ui.label(RichText::new("Eine kurze Rückmeldung hilft uns sehr beim Testen.").color(TEXT_DIM).size(13.0));
                    ui.add_space(10.0);
                    if ui.add(big_button("Es lief gut", GREEN.gamma_multiply(0.75)).min_size(Vec2::new(400.0, 46.0))).clicked() {
                        good = true;
                    }
                    if ui.add(big_button("Es gab Probleme", RED.gamma_multiply(0.6)).min_size(Vec2::new(400.0, 46.0))).clicked() {
                        bad = true;
                    }
                }
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Später").color(TEXT_DIM)).frame(false)).clicked() {
                        later = true;
                    }
                });
            });
        if good {
            self.save_feedback("gut");
            self.toasts.push(("Danke fürs Testen!".to_string(), 2, self.now));
            self.feedback = None;
        } else if bad {
            self.save_feedback(if err { "fehler" } else { "schlecht" });
            self.feedback = None;
            self.report_problem(ctx, if err { "Verbindung fehlgeschlagen" } else { "Problem beim Streamen" });
        } else if later {
            self.feedback = None;
        }
    }

    /// Lädt die Neuigkeiten (Release-Notizen) im Hintergrund.
    fn load_news(&mut self) {
        if self.news_loading {
            return;
        }
        self.news_loading = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let m = match update::fetch_news() {
                Ok(v) => Msg::News(v),
                Err(e) => Msg::NewsErr(e),
            };
            let _ = tx.send(m);
        });
    }

    /// Anzahl der Versionen, die der Nutzer noch nicht gesehen hat.
    fn unseen_news(&self) -> usize {
        let seen = if self.settings.news_seen.is_empty() { VERSION.to_string() } else { self.settings.news_seen.clone() };
        self.news.iter().filter(|n| update::is_newer(&n.version, &seen)).count()
    }

    /// Startet die Prüfung auf Updates im Hintergrund.
    fn check_updates(&mut self) {
        if self.update_checking {
            return;
        }
        self.update_checking = true;
        self.update_msg.clear();
        self.last_update_check = Instant::now();
        let tx = self.tx.clone();
        std::thread::spawn(move || match update::check(VERSION) {
            Ok(Some(r)) => {
                let _ = tx.send(Msg::UpdateAvail(r));
            }
            Ok(None) => {
                let _ = tx.send(Msg::UpdateNone);
            }
            Err(e) => {
                let _ = tx.send(Msg::UpdateCheckFailed(e));
            }
        });
    }

    /// Lädt und installiert das gefundene Update (mit Animation).
    fn start_update_install(&mut self) {
        let Some(rel) = self.update.clone() else { return };
        if self.update_busy {
            return;
        }
        if self.session.is_some() || self.starting.is_some() || self.reconnecting {
            self.toasts.push(("Während eines Spiels oder Verbindens wird nichts installiert. Bitte danach aktualisieren.".to_string(), 1, self.now));
            return;
        }
        self.update_busy = true;
        self.update_err.clear();
        *self.upd_progress.lock().unwrap() = update::Progress::default();
        self.upd_shown = 0.0;
        let tx = self.tx.clone();
        let prog = self.upd_progress.clone();
        std::thread::spawn(move || {
            if let Err(e) = update::install_ui(&rel, prog) {
                let _ = tx.send(Msg::UpdateErr(e));
            }
        });
    }

    fn keyboard_input(&self, ctx: &egui::Context, pad_connected: bool) -> pad::Input {
        let neutral = pad::Input { buttons: 0, l2: 0, r2: 0, lx: 0, ly: 0, rx: 0, ry: 0 };
        let off = self.settings.kb_mode == 2 || (self.settings.kb_mode == 0 && pad_connected);
        if off || self.stream_menu || self.show_keys || ctx.wants_keyboard_input() || !ctx.input(|i| i.focused) {
            return neutral;
        }
        ctx.input(|i| keys::eval(i, &self.settings.keys))
    }

    fn stream_ui(&mut self, ctx: &egui::Context) {
        let mut stop = false;
        let mut open_menu = false;
        let mut bar_shot = false;
        let mut bar_rec = false;
        let mut bar_keys = false;
        let mut bar_quick = false;
        let show_hud = self.show_hud;
        let muted_now = self.muted;
        let mut toggle_fs = false;
        let mut copy_log = false;
        let mut report = false;
        let opts = match self.settings.upscale {
            0 => TextureOptions::NEAREST,
            _ => TextureOptions::LINEAR,
        };
        // Eingabe sammeln (Controller + Tastatur)
        let pad_in = self.pad.poll();
        let pad_connected_now = self.pad.connected();
        let ds_on = self.settings.ds_features;
        let extra = if ds_on { self.pad.poll_extra() } else { pad::Extra::default() };
        let pad_chip = if pad_connected_now {
            let n = self.pad.name();
            let short: String = n.chars().take(18).collect();
            match self.pad.battery() {
                Some(b) => format!("{short} • Akku {b}"),
                None => short,
            }
        } else {
            String::new()
        };
        let kb = self.keyboard_input(ctx, pad_connected_now);
        let input = pad::Input {
            buttons: pad_in.buttons | kb.buttons,
            l2: pad_in.l2.max(kb.l2),
            r2: pad_in.r2.max(kb.r2),
            lx: if pad_in.lx != 0 { pad_in.lx } else { kb.lx },
            ly: if pad_in.ly != 0 { pad_in.ly } else { kb.ly },
            rx: pad_in.rx,
            ry: pad_in.ry,
        };
        let input = if self.stream_menu { pad::Input { buttons: 0, l2: 0, r2: 0, lx: 0, ly: 0, rx: 0, ry: 0 } } else { input };
        let pad_connected = self.pad.connected();
        let log_tail: String = self.log.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");

        let up = self.settings.upscale.min(3);
        let sharp = match up { 2 => self.settings.sharp, 3 => (self.settings.sharp + 0.35).min(1.0), _ => 0.0 };
        let nearest = up == 0;
        let con_info = self.session.as_ref().and_then(|s| self.consoles.iter().find(|c| c.name == s.name).map(|c| (c.is_ps5, c.running_app.clone())));
        let Some(s) = self.session.as_mut() else { return };
        let mut ie = ffi::OlInputEx { buttons: input.buttons, l2: input.l2, r2: input.r2, lx: input.lx, ly: input.ly, rx: input.rx, ry: input.ry, ..Default::default() };
        if ds_on {
            ie.touch_down = [extra.touch_down[0] as i32, extra.touch_down[1] as i32];
            ie.touch_x = extra.touch_x;
            ie.touch_y = extra.touch_y;
            ie.has_motion = extra.has_motion as i32;
            ie.gyro = extra.gyro;
            ie.accel = extra.accel;
            ie.ts_us = (self.now * 1_000_000.0) as u64 as u32;
        }
        s.stream.set_input_ex(&ie);
        if let Some(e) = s.stream.take_ctrl_events() {
            if ds_on {
                if e.trigger_new != 0 {
                    let inten = self.pad.ds_intensity;
                    self.pad.trigger_effects(e.type_left, &e.left, e.type_right, &e.right, inten);
                }
                if e.led_new != 0 {
                    self.pad.set_led(e.led[0], e.led[1], e.led[2]);
                }
                if e.player_new != 0 {
                    self.pad.set_player(e.player);
                }
            }
        }
        // Messwerte
        let (ping, dec, kbps) = s.stream.net_stats();
        s.ping = ping;
        s.dec_ms = dec;
        s.kbps = kbps;
        if s.hist_t.elapsed() >= Duration::from_millis(500) {
            s.hist_t = Instant::now();
            s.hist.push((ping, kbps as f32 / 1000.0, s.loss_rate));
            if s.hist.len() > 120 {
                s.hist.remove(0);
            }
        }
        if s.loss_t.elapsed() >= Duration::from_secs(1) {
            let lost_now = s.stream.status().8;
            s.loss_rate = (lost_now - s.loss_prev).max(0) as f32 / s.loss_t.elapsed().as_secs_f32();
            s.loss_prev = lost_now;
            s.loss_t = Instant::now();
        }
        if let Some((l, r)) = s.stream.take_rumble() {
            if self.settings.rumble {
                self.pad.rumble(l, r);
            }
        }
        s.last_input = s.last_input.max(Instant::now() - Duration::from_secs(3600));

        let (state, err, msg, frames, w, h, _rate, _ch, lost) = s.stream.status();
        s.state = state;
        s.ended_error = err;
        s.message = msg;
        s.size = (w, h);
        s.lost = lost;
        // Verbindung mitten im Spiel verloren: bis zu dreimal automatisch neu verbinden
        let has_video_now = s.has_video;
        let auto_retry = state == 2 && err && s.has_video && s.started.elapsed().as_secs() >= 20;
        let healthy = state == 1 && s.started.elapsed().as_secs() >= 60;
        if frames != s.frames_total {
            s.frames_total = frames;
        }
        if s.use_gpu && s.gpu.failed() {
            // Rückfall auf den alten Weg (CPU), falls die Grafikkarte den Shader ablehnt
            s.use_gpu = false;
            s.stream.set_rgba(true);
        }
        if s.use_gpu {
            if let Some((fw, fh)) = s.stream.take_nv12(&mut s.nv12) {
                {
                    let mut f = s.gpu.frame.lock().unwrap();
                    std::mem::swap(&mut f.nv12, &mut s.nv12);
                    f.w = fw;
                    f.h = fh;
                    f.dirty = true;
                    f.sharp = sharp;
                    f.nearest = nearest;
                    f.interp = self.settings.interp;
                }
                s.has_video = true;
                s.vsize = (fw, fh);
                s.fps_count += 1;
            }
        } else if let Some((fw, fh)) = s.stream.take_frame(&mut s.buf) {
            let img = egui::ColorImage::from_rgba_unmultiplied([fw, fh], &s.buf[..fw * fh * 4]);
            match s.tex.as_mut() {
                Some(t) => t.set(img, opts),
                None => s.tex = Some(ctx.load_texture("video", img, opts)),
            }
            s.has_video = true;
            s.vsize = (fw, fh);
            s.fps_count += 1;
        }
        if s.fps_t.elapsed() >= Duration::from_secs(1) {
            s.fps = s.fps_count;
            s.fps_count = 0;
            s.fps_t = Instant::now();
        }

        let activity = ctx.input(|i| {
            i.pointer.delta() != Vec2::ZERO
                || i.pointer.any_down()
                || i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
                || i.pointer.hover_pos().map(|p| p.y < 64.0).unwrap_or(false)
        });
        if activity {
            s.last_input = Instant::now();
        }
        let show_ui = s.last_input.elapsed() < Duration::from_secs(3) || s.state != 1 || self.stream_menu || self.show_keys || self.quick;
        if !show_ui && s.state == 1 {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }

        egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::BLACK)).show(ctx, |ui| {
            let full = ui.max_rect();
            if s.use_gpu && s.has_video {
                let (iw, ih) = (s.vsize.0 as f32, s.vsize.1 as f32);
                let scale = (full.width() / iw).min(full.height() / ih);
                let r = Rect::from_center_size(full.center(), Vec2::new(iw * scale, ih * scale));
                s.gpu.paint(ui.painter(), r);
            } else if let Some(tex) = &s.tex {
                let (iw, ih) = (tex.size()[0] as f32, tex.size()[1] as f32);
                let scale = (full.width() / iw).min(full.height() / ih);
                let r = Rect::from_center_size(full.center(), Vec2::new(iw * scale, ih * scale));
                ui.painter().image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }

            // Verbindungs-/Fehler-Overlay
            if s.state == 0 || s.state == 2 || !s.has_video {
                let ended = s.state == 2;
                egui::Area::new(egui::Id::new("overlay")).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    egui::Frame::none().fill(th_card()).rounding(20.0).stroke(Stroke::new(1.0, if ended && s.ended_error { RED.gamma_multiply(0.6) } else { th_blue().gamma_multiply(0.5) })).inner_margin(egui::Margin::same(28.0)).show(ui, |ui| {
                        ui.set_width(440.0);
                        ui.vertical_centered(|ui| {
                            if ended {
                                if s.ended_error {
                                    ui.label(RichText::new("!").size(44.0).strong().color(RED));
                                    ui.label(RichText::new("Verbindung beendet").size(20.0).strong());
                                } else {
                                    ui.label(RichText::new("Stream beendet").size(20.0).strong());
                                }
                                ui.label(RichText::new(german_reason(&mask_ips(&s.message))).color(TEXT_DIM));
                                if s.ended_error {
                                    ui.add_space(6.0);
                                    egui::Frame::none().fill(th_bg()).rounding(10.0).inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(RichText::new(&log_tail).monospace().size(11.0).color(TEXT_DIM));
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.button("Protokoll kopieren").clicked() {
                                            copy_log = true;
                                        }
                                        if ui.button("Problem melden").clicked() {
                                            report = true;
                                        }
                                    });
                                }
                                ui.add_space(8.0);
                                if ui.add(big_button("Zurück", th_blue()).min_size(Vec2::new(380.0, 46.0))).clicked() {
                                    stop = true;
                                }
                            } else {
                                ui.add(egui::Spinner::new().size(44.0));
                                ui.add_space(8.0);
                                ui.label(RichText::new(format!("Verbinde mit {} …", s.name)).size(20.0).strong());
                                let secs = s.started.elapsed().as_secs();
                                let sub = if !s.has_video && s.state == 1 { "Warte auf das erste Bild" } else { "Handshake mit der Konsole" };
                                ui.label(RichText::new(format!("{sub}  •  {secs}s")).color(TEXT_DIM));
                                if let Some(a) = &s.audio_err {
                                    ui.label(RichText::new(format!("Ton nicht verfügbar: {a}")).color(YELLOW).size(12.0));
                                }
                                ui.add_space(8.0);
                                if ui.add(big_button("Abbrechen", th_card_hover()).min_size(Vec2::new(380.0, 44.0))).clicked() {
                                    stop = true;
                                }
                            }
                        });
                    });
                });
            }

            // HUD + Steuerleiste
            if s.state == 1 && s.has_video {
                let atotal = s.stream.audio_total();
                let atxt = if muted_now {
                    "  •  Ton aus".to_string()
                } else {
                    match &s.audio {
                        None => "  •  Ton: kein Gerät".to_string(),
                        Some(a) => {
                            let err = a.shared.error.lock().map(|g| g.clone()).unwrap_or_default();
                            if !err.is_empty() {
                                "  •  Ton: Gerätefehler".to_string()
                            } else if atotal == 0 && s.started.elapsed() > Duration::from_secs(6) {
                                "  •  Ton: keine Daten von der Konsole".to_string()
                            } else {
                                "  •  Ton ok".to_string()
                            }
                        }
                    }
                };
                let items = self.settings.hud_items;
                let mut parts: Vec<String> = Vec::new();
                if items & (1 << 0) != 0 {
                    parts.push(format!("{}x{}", s.size.0, s.size.1));
                }
                if items & (1 << 1) != 0 {
                    parts.push(format!("{} FPS", s.fps));
                }
                if items & (1 << 2) != 0 && s.ping >= 0.0 {
                    parts.push(format!("Ping {:.0} ms", s.ping));
                }
                if items & (1 << 3) != 0 && s.ping >= 0.0 {
                    parts.push(format!("Dek. {:.1} ms", s.dec_ms));
                }
                if items & (1 << 4) != 0 && s.ping >= 0.0 {
                    parts.push(format!("{:.1} Mbit/s", s.kbps as f32 / 1000.0));
                }
                if items & (1 << 5) != 0 && s.lost > 0 {
                    parts.push(format!("{} Pakete verloren", s.lost));
                }
                if items & (1 << 6) != 0 && pad_connected {
                    parts.push("Controller".to_string());
                }
                if items & (1 << 7) != 0 {
                    parts.push(atxt.trim_start_matches("  •  ").to_string());
                }
                if items & (1 << 8) != 0 {
                    parts.push(winui::local_time_hm());
                }
                if items & (1 << 9) != 0 {
                    if let Some(b) = self.pad.battery() {
                        parts.push(format!("Akku {b}"));
                    }
                }
                let hud = parts.join("  •  ");
                if show_hud && !hud.is_empty() {
                    let bar_shift = 52.0 * ctx.animate_bool_with_time(egui::Id::new("bar_anim"), show_ui, 0.25);
                    paint_hud(ui.painter(), full, hud, &self.settings, bar_shift);
                }
                let anim = ctx.animate_bool_with_time(egui::Id::new("bar_anim"), show_ui, 0.25);
                if anim > 0.01 {
                    let (ps5, game) = con_info.clone().unwrap_or((true, String::new()));
                    let no_audio = !muted_now && (atotal == 0 && s.started.elapsed() > Duration::from_secs(6) || s.audio.is_none());
                    let quality = format!("{}p • {} FPS", s.size.1, s.fps);
                    let stats = BarStats { update: self.update.as_ref().map(|r| r.version.clone()), ping: s.ping, dec: s.dec_ms, mbit: s.kbps as f32 / 1000.0, loss: s.loss_rate, pad: pad_chip.clone() };
                    let fl = draw_stream_bar(ctx, anim, self.now, ps5, &game, &quality, if no_audio { Some(atxt.trim_start_matches("  •  ")) } else { None }, self.muted, &mut self.settings.volume, &stats);
                    if fl.menu {
                        open_menu = true;
                    }
                    if fl.quick {
                        bar_quick = true;
                    }
                    if fl.fullscreen {
                        toggle_fs = true;
                    }
                    if fl.keys {
                        bar_keys = true;
                    }
                    if fl.shot {
                        bar_shot = true;
                    }
                    if fl.rec {
                        bar_rec = true;
                    }
                    if fl.mute {
                        self.muted = !self.muted;
                    }
                }
            }
        });

        if bar_shot {
            self.shot_requested = true;
        }
        if bar_rec {
            self.rec_toggle = true;
        }
        if bar_keys {
            self.show_keys = !self.show_keys;
        }
        if bar_quick {
            self.quick = !self.quick;
        }
        if ctx.input(|i| i.key_pressed(Key::F11)) {
            toggle_fs = true;
        }
        if ctx.input(|i| i.key_pressed(Key::M)) && !self.stream_menu && !ctx.wants_keyboard_input() {
            self.muted = !self.muted;
            self.toasts.push((if self.muted { "Ton aus (M)".to_string() } else { "Ton an".to_string() }, 0, self.now));
        }
        let gain = if self.muted { 0.0 } else { self.settings.volume };
        if let Some(s) = self.session.as_ref() {
            if let Some(a) = &s.audio {
                a.shared.set_gain(gain);
                a.shared.set_eq(self.settings.eq_bass, self.settings.eq_mid, self.settings.eq_treble);
            }
        }
        let want_shot = ctx.input(|i| i.key_pressed(Key::F12)) || self.shot_requested;
        self.shot_requested = false;
        if want_shot {
            let msg = match self.session.as_ref() {
                Some(s) if s.use_gpu && s.has_video => {
                    let f = s.gpu.frame.lock().unwrap();
                    let rgba = video::nv12_to_rgba(&f.nv12, f.w, f.h);
                    save_screenshot(&rgba, f.w, f.h)
                }
                Some(s) => match &s.tex {
                    Some(tex) => save_screenshot(&s.buf, tex.size()[0], tex.size()[1]),
                    None => Err("Noch kein Bild vorhanden".to_string()),
                },
                None => Err("Kein Stream".to_string()),
            };
            match msg {
                Ok(path) => self.toasts.push((format!("Screenshot gespeichert: {path}"), 2, self.now)),
                Err(e) => self.toasts.push((format!("Screenshot fehlgeschlagen: {e}"), 1, self.now)),
            }
        }

        // Aufnahme: F9 oder Knopf
        let want_rec = ctx.input(|i| i.key_pressed(Key::F9)) && !ctx.wants_keyboard_input() || self.rec_toggle;
        self.rec_toggle = false;
        if want_rec {
            if let Some(s) = self.session.as_ref() {
                if s.stream.record_state() != 0 {
                    s.stream.record_stop();
                    let name = std::mem::take(&mut self.rec_name);
                    self.rec_started = None;
                    self.toasts.push((format!("Aufnahme gespeichert: {name}"), 2, self.now));
                } else {
                    match recording_path() {
                        Ok((full, name)) => match s.stream.record_start(&full) {
                            Ok(()) => {
                                self.rec_started = Some(Instant::now());
                                self.rec_name = name;
                                self.toasts.push(("Aufnahme läuft. Mit F9 beendest du sie.".to_string(), 0, self.now));
                            }
                            Err(e) => self.toasts.push((format!("Aufnahme nicht möglich: {e}"), 1, self.now)),
                        },
                        Err(e) => self.toasts.push((format!("Aufnahme nicht möglich: {e}"), 1, self.now)),
                    }
                }
            }
        }
        // Anzeige "REC" mit Zeit und Dateigröße
        if let (Some(t0), Some(s)) = (self.rec_started, self.session.as_ref()) {
            if s.stream.record_state() == 0 {
                self.rec_started = None;
            } else {
                let secs = t0.elapsed().as_secs();
                let mb = s.stream.record_bytes() as f64 / 1_048_576.0;
                let pulse = 0.55 + 0.45 * ((self.now * 3.0).sin() as f32 * 0.5 + 0.5);
                let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("rec_badge")));
                let txt = format!("REC  {:02}:{:02}  {:.0} MB", secs / 60, secs % 60, mb);
                let galley = p.layout_no_wrap(txt, FontId::proportional(14.0), Color32::WHITE);
                let r = Rect::from_min_size(Pos2::new(16.0, 76.0), galley.size() + Vec2::new(34.0, 12.0));
                p.rect_filled(r, Rounding::same(14.0), Color32::from_black_alpha(170));
                p.circle_filled(Pos2::new(r.min.x + 15.0, r.center().y), 5.0, Color32::from_rgb(235, 60, 60).gamma_multiply(pulse));
                p.galley(Pos2::new(r.min.x + 27.0, r.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        }
        let (esc, f1, f10) = ctx.input(|i| (i.key_pressed(Key::Escape), i.key_pressed(Key::F1), i.key_pressed(Key::F10)));
        let state_now = self.session.as_ref().map(|s| s.state).unwrap_or(0);
        if esc {
            if self.quick {
                self.quick = false;
            } else if self.show_keys {
                self.show_keys = false;
            } else if state_now == 1 {
                self.stream_menu = !self.stream_menu;
                self.close_hint = false;
            }
        }
        if f1 && state_now == 1 {
            self.show_keys = !self.show_keys;
        }
        if f10 {
            self.show_hud = !self.show_hud;
        }
        if open_menu {
            self.stream_menu = true;
        }
        let apply_quality = self.quick_panel(ctx);
        let (o_resume, o_fs, o_keys, o_stop, o_quit) = self.stream_overlays(ctx);
        if o_resume {
            self.stream_menu = false;
            self.close_hint = false;
        }
        if o_fs {
            toggle_fs = true;
        }
        if o_keys {
            self.stream_menu = false;
            self.show_keys = true;
        }
        if o_stop {
            stop = true;
        }
        if o_quit {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if healthy {
            self.auto_retries = 0;
        }
        if state == 1 && has_video_now {
            self.quality_retry = 0;
        }
        if self.quality_retry > 0 && state == 2 && err && !has_video_now && !stop {
            if let Some((c, p, _)) = self.last_connect.clone() {
                self.quality_retry -= 1;
                self.stop_stream(ctx);
                self.feedback = None;
                self.reconnecting = true;
                self.reconnect_delay_ms = 4000;
                self.reconnect_msg = "Die Konsole braucht einen Moment, neuer Versuch …".to_string();
                self.pending_reconnect = Some((c, p, Instant::now()));
                return;
            }
        }
        if auto_retry && !stop && self.auto_retries < 3 {
            if let Some((c, p, _)) = self.last_connect.clone() {
                self.auto_retries += 1;
                let n = self.auto_retries;
                self.stop_stream(ctx);
                self.feedback = None;
                self.reconnecting = true;
                self.reconnect_msg = format!("Verbindung verloren, verbinde neu ({n} von 3) …");
                self.reconnect_delay_ms = 2500;
                self.pending_reconnect = Some((c, p, Instant::now()));
                return;
            }
        }
        if apply_quality {
            if let Some((c, p, _)) = self.last_connect.clone() {
                self.quick = false;
                self.stop_stream(ctx);
                self.feedback = None;
                self.reconnecting = true;
                self.reconnect_msg = "Neue Qualität wird angewendet …".to_string();
                self.quality_retry = 3;
                self.reconnect_delay_ms = 4500;
                self.pending_reconnect = Some((c, p, Instant::now()));
            }
        }
        if copy_log {
            ctx.copy_text(self.log.clone());
        }
        if report {
            self.report_problem(ctx, "Verbindung fehlgeschlagen");
        }
        if toggle_fs {
            self.fullscreen = !self.fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        }
        if stop {
            self.stop_stream(ctx);
        }
    }
}

impl eframe::App for App {
    /// Die Menüs werden bewusst nicht per Controller bedient (Controller-Test in der Diagnose).
    fn raw_input_hook(&mut self, _ctx: &egui::Context, _raw: &mut egui::RawInput) {
        self.nav_prev = 0;
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.gl_ok = frame.gl().is_some();
        lang::set(self.settings.language);
        ctx.input(|i| {
            let v = i.viewport();
            if let (Some(o), Some(inn)) = (v.outer_rect, v.inner_rect) {
                if let Ok(mut g) = update::WIN_RECT.lock() {
                    *g = Some([o.min.x, o.min.y, inn.width(), inn.height(), if v.maximized.unwrap_or(false) { 1.0 } else { 0.0 }]);
                }
            }
        });
        if self.news_lang != self.settings.language {
            let first = self.news_lang == usize::MAX;
            self.news_lang = self.settings.language;
            if !first {
                // Sprache gewechselt: Neuigkeiten in der neuen Sprache laden
                self.load_news();
            }
        }
        if self.gl_info.is_empty() {
            if let Some(gl) = frame.gl() {
                use eframe::glow::HasContext;
                let (r, v) = unsafe { (gl.get_parameter_string(eframe::glow::RENDERER), gl.get_parameter_string(eframe::glow::VERSION)) };
                self.gl_info = format!("{r} (OpenGL {v})");
            }
        }
        let t = ctx.input(|i| i.time);
        self.now = t;
        {
            let z = [0.88f32, 1.0, 1.12][self.settings.ui_scale.min(2)];
            if (ctx.zoom_factor() - z).abs() > 0.001 {
                ctx.set_zoom_factor(z);
            }
        }
        if (t * 2.0) as u64 % 4 == 0 {
            self.pad_name_cache = if self.pad.connected() { self.pad.name() } else { String::new() };
        }
        if self.titlebar_dirty && self.title_tries < 30 {
            self.title_tries += 1;
            let th = theme::get();
            winui::style_titlebar("OVERLIME Remote Play", th.sidebar, th.accent);
            if self.title_tries >= 3 {
                self.titlebar_dirty = false;
                self.title_tries = 0;
            }
        }
        if self.applied_theme != self.settings.theme {
            if let Some(p) = self.themes.iter().find(|p| p.name == self.settings.theme) {
                theme::set(p.theme);
            }
            setup_style(ctx);
            self.applied_theme = self.settings.theme.clone();
            self.titlebar_dirty = true;
        }
        // Nie aus Versehen mitten im Stream schließen (X-Knopf, Alt+F4)
        if self.session.is_some() && !self.allow_close && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.stream_menu = true;
            self.close_hint = true;
        }
        self.handle_messages(ctx);
        self.pump_log();
        self.process_capture(ctx, t);
        if let Some((_, t0)) = &self.audio_test {
            if t - *t0 > 1.6 {
                self.audio_test = None;
            }
        }
        if self.session.is_none() && self.dialog.is_none() && self.capturing.is_none() && !ctx.wants_keyboard_input() {
            let (f5, nums) = ctx.input(|i| {
                let c = i.modifiers.ctrl;
                let ks = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5];
                (i.key_pressed(Key::F5), ks.map(|k| c && i.key_pressed(k)))
            });
            if f5 && !self.searching {
                self.search();
            }
            let hk = [Page::Home, Page::News, Page::Consoles, Page::Settings, Page::Help];
            for (i, hit) in nums.iter().enumerate() {
                if *hit {
                    self.page = hk[i];
                }
            }
        }

        if self.settings != self.saved {
            self.settings.save();
            self.saved = self.settings.clone();
        }

        self.update_discord();
        if std::env::args().any(|a| a == "--demo-pad") {
            let conn = self.pad.connected();
            let inp = self.pad.poll();
            let ex = self.pad.poll_extra();
            let devs = self.pad.devices();
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add_space(10.0);
                pages::pad_visual(ui, &inp, &ex, conn);
                ui.add_space(8.0);
                for (n, u, v, p) in devs {
                    ui.label(format!("{n}  ({v:04x}:{p:04x})  {}", if u { "als Gamepad nutzbar" } else { "nicht nutzbar" }));
                }
            });
            ctx.request_repaint();
            return;
        }
        if std::env::args().any(|a| a == "--demo-video") {
            // Testbild über denselben Grafikkarten-Weg wie der Stream
            let mode: usize = std::env::args().find_map(|a| a.strip_prefix("--mode=").and_then(|v| v.parse().ok())).unwrap_or(2);
            let gpu = self.demo_gpu.get_or_insert_with(|| {
                let g = video::VideoGpu::new();
                {
                    let mut f = g.frame.lock().unwrap();
                    let sw: usize = std::env::args().find_map(|a| a.strip_prefix("--demo-src=").and_then(|v| v.parse().ok())).unwrap_or(1280);
                    let sh = sw * 9 / 16 / 2 * 2;
                    f.w = sw;
                    f.h = sh;
                    f.nv12 = video::test_pattern_nv12(sw, sh);
                    f.interp = std::env::args().any(|a| a == "--demo-interp");
                    f.force_t = std::env::args().find_map(|a| a.strip_prefix("--force-t=").and_then(|v| v.parse().ok()));
                    f.dirty = true;
                }
                g
            }).clone();
            {
                let mut f = gpu.frame.lock().unwrap();
                f.sharp = [0.0, 0.0, 0.55, 0.9][mode.min(3)];
                f.nearest = mode == 0;
            }
            if std::env::args().any(|a| a == "--demo-interp") {
                // Quelle mit 10 Bildern pro Sekunde, die jedes Mal 24 Pixel nach rechts springt
                let (w, h) = {
                    let f = gpu.frame.lock().unwrap();
                    (f.w, f.h)
                };
                if self.demo_base.is_empty() {
                    self.demo_base = video::test_pattern_nv12(w, h);
                }
                let freeze = std::env::args().any(|a| a == "--freeze");
                // mit --freeze: genau zwei Bilder (Schritt 2 und 3), danach Stillstand
                let step = if freeze { if self.demo_step == usize::MAX { 2 } else { 3 } } else { (self.now * 10.0) as usize };
                if step != self.demo_step && !(freeze && self.demo_step == 3) {
                    self.demo_step = step;
                    let mut f = gpu.frame.lock().unwrap();
                    f.nv12 = video::shift_nv12(&self.demo_base, w, h, (step * 24) % w);
                    f.dirty = true;
                }
            }
            let failed = gpu.failed();
            egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::BLACK)).show(ctx, |ui| {
                let full = ui.max_rect();
                let scale = (full.width() / 1280.0).min(full.height() / 720.0);
                let r = Rect::from_center_size(full.center(), Vec2::new(1280.0 * scale, 720.0 * scale));
                gpu.paint(ui.painter(), r);
                if failed {
                    ui.painter().text(full.center(), Align2::CENTER_CENTER, format!("GPU-Fehler: {}", gpu.error.lock().unwrap()), FontId::proportional(16.0), RED);
                }
            });
            ctx.request_repaint();
            return;
        }
        if std::env::args().any(|a| a == "--demo-bar") {
            egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::BLACK)).show(ctx, |_| {});
            let mut vol = 1.0f32;
            draw_stream_bar(ctx, 1.0, t, true, "EA SPORTS FC 26", "1080p • 60 FPS", Some("Ton: keine Daten von der Konsole"), false, &mut vol, &BarStats { update: Some("0.20".to_string()), ping: 11.0, dec: 3.2, mbit: 27.4, loss: 0.0, pad: "DualSense • Akku voll".to_string() });
            if std::env::args().any(|a| a == "--demo-quick") {
                self.quick = true;
                self.quick_panel(ctx);
            }
            if std::env::args().any(|a| a == "--demo-hud") {
                let mask = self.settings.hud_items;
                let demo_parts = ["1920x1080", "60 FPS", "Ping 11 ms", "Dek. 3.2 ms", "27.4 Mbit/s", "4 Pakete verloren", "Controller", "Ton ok", "21:45", "Akku voll"];
                let hud: Vec<&str> = demo_parts.iter().enumerate().filter(|(i, _)| mask & (1 << i) != 0).map(|(_, s)| *s).collect();
                paint_hud(&ctx.layer_painter(egui::LayerId::background()), ctx.screen_rect(), hud.join("  •  "), &self.settings, 0.0);
            }
            if std::env::args().any(|a| a == "--demo-toast") {
                if self.toasts.is_empty() {
                    self.toasts.push(("Update 0.27 verfügbar – nach dem Spiel installieren".to_string(), 0, self.now));
                    self.toasts.push(("Screenshot gespeichert: OVERLIME-1.png".to_string(), 2, self.now));
                }
                self.draw_toasts(ctx, t);
            }
            if std::env::args().any(|a| a == "--demo-pause") {
                self.stream_menu = true;
                let _ = self.stream_overlays(ctx);
            }
            ctx.request_repaint();
            return;
        }
        if self.update_click {
            self.update_click = false;
            self.start_update_install();
        }
        if self.check_click {
            self.check_click = false;
            self.check_updates();
        }
        if self.last_update_check.elapsed() > Duration::from_secs(20 * 60) {
            self.check_updates();
        }
        if self.news_fetched.map(|t| t.elapsed() > Duration::from_secs(60 * 60)).unwrap_or(false) {
            self.load_news();
        }
        // Neuigkeiten gelesen, sobald die Seite offen ist
        if self.page == Page::News && !self.news.is_empty() {
            let latest = self.news[0].version.clone();
            if self.settings.news_seen != latest {
                self.settings.news_seen = latest;
            }
        }
        // Qualität angewendet: kurz neu verbinden
        if self.reconnecting && self.session.is_none() {
            ctx.request_repaint();
            egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::BLACK)).show(ctx, |ui| {
                let r = ui.max_rect();
                let c = r.center();
                ui.painter().text(c + Vec2::new(0.0, 40.0), Align2::CENTER_CENTER, if self.reconnect_msg.is_empty() { "Verbinde neu …" } else { &self.reconnect_msg }, FontId::proportional(20.0), Color32::WHITE);
                ui.painter().text(c + Vec2::new(0.0, 72.0), Align2::CENTER_CENTER, "OVERLIME verbindet kurz neu", FontId::proportional(13.0), TEXT_DIM);
                ui.put(Rect::from_center_size(c - Vec2::new(0.0, 20.0), Vec2::splat(44.0)), egui::Spinner::new().size(44.0));
            });
            let go = self.pending_reconnect.as_ref().map(|(_, _, t0)| t0.elapsed() > Duration::from_millis(self.reconnect_delay_ms)).unwrap_or(false);
            if go {
                if let Some((c, p, _)) = self.pending_reconnect.take() {
                    self.connect(&c, &p);
                }
            }
            return;
        }
        // Update-Animation
        if let Some(from) = self.post_update.clone() {
            let t0 = *self.post_t0.get_or_insert_with(|| {
                // Das Endbild des alten Fensters übernehmen: Stufe "Neustart" ist schon eine Weile aktiv
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("upd_stage_t"), (4u8, t - 2.6)));
                t
            });
            ctx.request_repaint();
            let mut prog = update::Progress::default();
            prog.stage = 4;
            prog.pct = 100.0;
            splash::update_screen(ctx, t, &prog, 1.0, &from, VERSION);
            // Nach dem ersten gezeichneten Bild meldet sich das neue Fenster beim alten
            if t - t0 > 0.05 {
                update::signal_ready();
            }
            if t - t0 > 1.0 {
                self.post_update = None;
            }
            return;
        }
        if self.update_busy && self.update.is_some() {
            ctx.request_repaint();
            let prog = self.upd_progress.lock().unwrap().clone();
            let target = if prog.error.is_some() {
                self.upd_shown
            } else if prog.stage >= 4 {
                1.0
            } else {
                (prog.stage as f32 + if prog.stage == 1 { prog.pct / 100.0 } else { 0.5 }) / 5.0
            };
            let dt = ctx.input(|i| i.stable_dt).min(0.1);
            self.upd_shown += (target - self.upd_shown) * (dt * 2.2).min(1.0);
            let rel = self.update.clone().unwrap();
            if splash::update_screen(ctx, t, &prog, self.upd_shown, VERSION, &rel.version) {
                self.update_busy = false;
                self.update_err = prog.error.unwrap_or_default();
                *self.upd_progress.lock().unwrap() = update::Progress::default();
            }
            return;
        }
        if self.session.is_none() {
            if let Some((from, to)) = self.welcome.clone() {
                ctx.request_repaint();
                let start = *self.welcome_start.get_or_insert(t);
                if splash::welcome_screen(ctx, t - start, &from, &to) {
                    self.welcome = None;
                }
                return;
            }
        }

        if self.session.is_some() {
            ctx.request_repaint();
            if !self.searching && self.last_scan.elapsed() > Duration::from_secs(15) {
                self.search();
            }
            self.stream_ui(ctx);
            // Hinweise (Update, Screenshot, Aufnahme …) auch während des Spiels anzeigen
            self.draw_toasts(ctx, t);
            return;
        }

        // Erster Start: erst anmelden
        if std::env::args().any(|a| a == "--login") || (!self.skip_login && psn::id_from_b64(&self.settings.psn_account).is_none() && !std::env::args().any(|a| a == "--demo")) {
            ctx.request_repaint_after(Duration::from_millis(33));
            self.login_screen(ctx, t);
            return;
        }

        if !self.searching && self.last_scan.elapsed() > Duration::from_secs(5) {
            self.search();
        }
        if self.settings != self.saved {
            self.settings.save();
            self.saved = self.settings.clone();
        }
        {
            // Im Leerlauf sparsam zeichnen: 30 Bilder/s im Vordergrund, selten wenn unscharf oder minimiert
            let (focused, minimized) = ctx.input(|i| (i.viewport().focused.unwrap_or(true), i.viewport().minimized.unwrap_or(false)));
            let animated = matches!(self.page, Page::Home | Page::Consoles);
            let changing = t - self.page_since < 0.5;
            let ms = if minimized { 1000 } else if !focused { 250 } else if changing { 16 } else if animated { 50 } else { 100 };
            ctx.request_repaint_after(Duration::from_millis(ms));
        }

        // animierter Hintergrund
        {
            let r = ctx.screen_rect();
            let p = ctx.layer_painter(egui::LayerId::background());
            p.rect_filled(r, Rounding::ZERO, th_bg());
            let a = (t * 0.25) as f32;
            p.circle_filled(Pos2::new(r.width() * (0.75 + 0.08 * a.sin()), r.height() * (0.2 + 0.06 * (a * 1.3).cos())), 260.0, th_blue().gamma_multiply(0.07));
            p.circle_filled(Pos2::new(r.width() * (0.35 + 0.07 * (a * 0.8).cos()), r.height() * (0.85 + 0.05 * a.sin())), 300.0, Color32::from_rgb(90, 60, 220).gamma_multiply(0.05));
        }

        egui::TopBottomPanel::top("topbar")
            .exact_height(46.0)
            .frame(egui::Frame::none().fill(th_sidebar()).inner_margin(egui::Margin::symmetric(20.0, 0.0)))
            .show(ctx, |ui| {
                let (page_icon, page_title) = match self.page {
                    Page::Home => (ic::HOUSE, "Übersicht"),
                    Page::News => (ic::NEWSPAPER, "Neuigkeiten"),
                    Page::Consoles => (ic::GAME_CONTROLLER, "Konsolen"),
                    Page::Settings => (ic::GEAR_SIX, "Einstellungen"),
                    Page::Help => (ic::QUESTION, "Hilfe & Diagnose"),
                };
                let found = self.consoles.len();
                let pad_ok = self.pad.connected();
                let pad_txt = if pad_ok { let n = self.pad.name(); if n.is_empty() { "Controller".to_string() } else { n.chars().take(22).collect() } } else { "Kein Controller".to_string() };
                let logged = psn::id_from_b64(&self.settings.psn_account).is_some();
                let acc_txt = if logged { if self.settings.psn_name.is_empty() { "Angemeldet".to_string() } else { self.settings.psn_name.clone() } } else { "Nicht angemeldet".to_string() };
                let searching = self.searching;
                let upd_ver = self.update.as_ref().map(|r| r.version.clone());
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("OVERLIME").size(13.0).strong().color(TEXT_DIM));
                    ui.label(RichText::new("/").size(13.0).color(TEXT_DIM.gamma_multiply(0.6)));
                    ui.label(RichText::new(page_icon).size(16.0).color(th_blue_light()));
                    ui.label(RichText::new(page_title).size(15.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(v) = &upd_ver {
                            let b = egui::Button::new(RichText::new(format!("{}  Update {v}", ic::ARROW_CIRCLE_UP)).size(12.5).color(Color32::WHITE)).fill(th_blue()).rounding(14.0).min_size(Vec2::new(0.0, 28.0));
                            if ui.add(b).on_hover_text("Zu den Updates").clicked() {
                                self.page = Page::Settings;
                            }
                        }
                        top_chip(ui, ic::USER_CIRCLE, &acc_txt, if logged { GREEN } else { YELLOW });
                        top_chip(ui, ic::GAME_CONTROLLER, &pad_txt, if pad_ok { GREEN } else { TEXT_DIM });
                        let (ctxt, ccol) = if searching && found == 0 { ("Suche …".to_string(), th_blue_light()) } else if found == 0 { ("Keine Konsole".to_string(), TEXT_DIM) } else if found == 1 { ("1 Konsole".to_string(), GREEN) } else { (format!("{found} Konsolen"), GREEN) };
                        top_chip(ui, ic::WIFI_HIGH, &ctxt, ccol);
                    });
                });
                let r = ui.max_rect();
                ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, Color32::from_white_alpha(14)));
            });

        egui::SidePanel::left("nav")
            .exact_width(210.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(th_sidebar()).inner_margin(egui::Margin::same(16.0)))
            .show(ctx, |ui| {
                ui.add_space(6.0);
                let (lr, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.0), egui::Sense::hover());
                {
                    let p = ui.painter_at(lr);
                    p.rect_filled(Rect::from_min_size(lr.min + Vec2::new(0.0, 6.0), Vec2::new(5.0, 44.0)), Rounding::same(3.0), th_blue_light());
                    p.text(lr.min + Vec2::new(16.0, 4.0), Align2::LEFT_TOP, "OVERLIME", FontId::proportional(27.0), Color32::WHITE);
                    p.text(lr.min + Vec2::new(17.0, 36.0), Align2::LEFT_TOP, "REMOTE PLAY", FontId::proportional(12.0), th_blue_light());
                }
                ui.add_space(18.0);
                let old = self.page;
                let pages = [Page::Home, Page::News, Page::Consoles, Page::Settings, Page::Help];
                let items = [(ic::HOUSE, "Übersicht"), (ic::NEWSPAPER, "Neuigkeiten"), (ic::GAME_CONTROLLER, "Konsolen"), (ic::GEAR_SIX, "Einstellungen"), (ic::QUESTION, "Hilfe")];
                let unseen = self.unseen_news();
                let top_y = ui.cursor().top();
                let sel_idx = pages.iter().position(|p| *p == self.page).unwrap_or(0) as f32;
                let hy = ui.ctx().animate_value_with_time(egui::Id::new("nav_hl"), sel_idx, 0.3);
                {
                    let w = ui.available_width();
                    let rect = Rect::from_min_size(Pos2::new(ui.cursor().left(), top_y + hy * 54.0), Vec2::new(w, 44.0));
                    ui.painter().rect_filled(rect, Rounding::same(12.0), th_card_hover().gamma_multiply(0.8));
                    let bar = Rect::from_min_size(rect.min + Vec2::new(0.0, 10.0), Vec2::new(4.0, 24.0));
                    ui.painter().rect_filled(bar, Rounding::same(2.0), th_blue_light());
                }
                for (i, (icon, label)) in items.iter().enumerate() {
                    let badge = if i == 3 && self.update.is_some() {
                        Some("!".to_string())
                    } else if i == 1 && unseen > 0 && self.page != Page::News {
                        Some(unseen.min(9).to_string())
                    } else {
                        None
                    };
                    if nav_item(ui, icon, label, self.page == pages[i], badge) {
                        self.page = pages[i];
                    }
                }
                if old != self.page {
                    self.page_since = t;
                }
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 118.0), egui::Sense::hover());
                    let p = ui.painter_at(rect);
                    p.rect_filled(rect, Rounding::same(16.0), th_card());
                    let logged = psn::id_from_b64(&self.settings.psn_account).is_some();
                    let name = if !logged { "Nicht angemeldet".to_string() } else if self.settings.psn_name.is_empty() { "PlayStation-Konto".to_string() } else { self.settings.psn_name.clone() };
                    // Avatar mit Anfangsbuchstabe
                    let av = rect.min + Vec2::new(30.0, 30.0);
                    p.circle_filled(av, 20.0, if logged { th_blue() } else { Color32::from_white_alpha(22) });
                    p.circle_stroke(av, 20.0, Stroke::new(1.5, if logged { th_blue_light() } else { Color32::from_white_alpha(40) }));
                    let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
                    p.text(av, Align2::CENTER_CENTER, if logged { initial } else { ic::USER.to_string() }, FontId::proportional(18.0), Color32::WHITE);
                    p.text(rect.min + Vec2::new(60.0, 21.0), Align2::LEFT_CENTER, name, FontId::proportional(13.5), Color32::WHITE);
                    let (sub, subc) = if logged { ("PlayStation-Konto", GREEN) } else { ("Anmeldung offen", YELLOW) };
                    p.text(rect.min + Vec2::new(60.0, 40.0), Align2::LEFT_CENTER, sub, FontId::proportional(11.5), subc);
                    p.hline((rect.min.x + 14.0)..=(rect.max.x - 14.0), rect.min.y + 60.0, Stroke::new(1.0, Color32::from_white_alpha(14)));
                    // Qualität und Netzwerk
                    let online = self.local_ip != "?";
                    let pulse = 0.55 + 0.45 * (self.now * 2.5).sin().abs() as f32;
                    p.circle_filled(rect.min + Vec2::new(22.0, 78.0), 4.0, if online { GREEN.gamma_multiply(pulse) } else { RED });
                    p.text(rect.min + Vec2::new(34.0, 78.0), Align2::LEFT_CENTER, if online { "Online" } else { "Offline" }, FontId::proportional(11.5), TEXT_DIM);
                    let q = format!("{} • {} FPS", ["540p", "720p", "1080p"][self.settings.resolution], [30, 60][self.settings.fps]);
                    p.text(rect.min + Vec2::new(16.0, 98.0), Align2::LEFT_CENTER, q, FontId::proportional(11.5), th_blue_light());
                    let (vt, vc) = if self.update.is_some() { (format!("v{VERSION} • Update"), YELLOW) } else if self.update_checked { (format!("v{VERSION} • aktuell"), GREEN) } else { (format!("v{VERSION}"), TEXT_DIM) };
                    p.text(Pos2::new(rect.max.x - 14.0, rect.min.y + 78.0), Align2::RIGHT_CENTER, vt, FontId::proportional(11.5), vc);
                });
            });

        egui::CentralPanel::default().frame(egui::Frame::none().inner_margin(egui::Margin::same(28.0))).show(ctx, |ui| {
            ui.set_opacity(fade(t, self.page_since, 0.3));
            ui.add_space((1.0 - fade(t, self.page_since, 0.4)) * 18.0);
            if let Some(rel) = self.update.clone() {
                egui::Frame::none().fill(th_blue().gamma_multiply(0.22)).rounding(14.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.6))).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("Update {} verfügbar", rel.version)).strong());
                            if !self.update_err.is_empty() {
                                ui.label(RichText::new(&self.update_err).color(RED).size(12.0));
                            } else {
                                ui.label(RichText::new("Die App lädt es herunter, prüft es und startet neu.").color(TEXT_DIM).size(12.0));
                            }
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if self.update_busy {
                                ui.spinner();
                            } else if ui.add(big_button("Jetzt aktualisieren", th_blue()).min_size(Vec2::new(170.0, 38.0))).clicked() {
                                self.update_busy = true;
                                self.update_err.clear();
                                let tx = self.tx.clone();
                                let r = rel.clone();
                                *self.upd_progress.lock().unwrap() = update::Progress::default();
                                self.upd_shown = 0.0;
                                let prog = self.upd_progress.clone();
                                std::thread::spawn(move || {
                                    if let Err(e) = update::install_ui(&r, prog) {
                                        let _ = tx.send(Msg::UpdateErr(e));
                                    }
                                });
                            }
                        });
                    });
                });
                ui.add_space(10.0);
            }
            match self.page {
                Page::Home => self.home_page(ui, t),
                Page::News => self.news_page(ui, t),
                Page::Consoles => self.consoles_page(ui, t),
                Page::Settings => self.settings_page(ui),
                Page::Help => self.help_page(ui, t),
            }
        });

        if self.dialog.is_none() {
            self.feedback_ui(ctx);
        }
        if !self.tour_checked && self.now > 2.0 {
            self.tour_checked = true;
            let ready = !self.settings.tour_done && self.session.is_none() && self.dialog.is_none() && !self.report_open && !self.update_busy
                && !std::env::args().any(|a| a == "--demo" || a == "--login" || a.starts_with("--demo-"));
            let force = std::env::args().any(|a| a == "--tour");
            if ready || force {
                self.tour = Some(0);
            }
        }
        self.dialog_ui(ctx);
        self.report_ui(ctx);
        self.tour_ui(ctx);
        self.draw_toasts(ctx, t);
    }
}

fn selftest() {
    use std::io::Write;
    let out = std::env::temp_dir().join("overlime-selftest.txt");
    let mut f = std::fs::File::create(&out).unwrap();
    let _ = writeln!(f, "init ok");
    let r = ffi::register("127.0.0.1", "09000000", true, [1, 2, 3, 4, 5, 6, 7, 8], 12345678, "test");
    let _ = writeln!(f, "register -> {:?}", r.map(|p| p.nickname));
    let paired = Paired { console_id: "x".into(), ps5: true, nickname: "t".into(), rp_regist_key: [b'a'; 16], rp_key: [1; 16] };
    let params = StreamParams { host: "127.0.0.1".into(), paired, width: 1920, height: 1080, fps: 60, bitrate_kbps: 30000, hevc: true, hw_decode: true, dualsense: false };
    match Stream::start(&params) {
        Ok(s) => {
            let _ = writeln!(f, "stream started");
            for _ in 0..40 {
                std::thread::sleep(Duration::from_millis(250));
                let st = s.status();
                let _ = writeln!(f, "status state={} err={} msg={}", st.0, st.1, st.2);
                if st.0 == 2 {
                    break;
                }
            }
        }
        Err(e) => {
            let _ = writeln!(f, "stream start error: {e}");
        }
    }
    let _ = writeln!(f, "mask test: {}", mask_ips("Trying 127.0.0.1:9295 und 192.168.178.26, v1.2.3 ok 12.5"));
    let _ = writeln!(f, "--- log ---
{}", mask_ips(&ffi::take_log()));
    let _ = writeln!(f, "done");
}

fn discord_test() {
    use std::io::Write;
    let out = std::env::temp_dir().join("overlime-discord-test.txt");
    let mut f = std::fs::File::create(&out).unwrap();
    let mut h = discord::spawn();
    let st = Settings::default();
    let (details, state) = presence_lines(&st, 2, "EA SPORTS FC 26", true, 1);
    let start = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let secs: u64 = std::env::args().skip_while(|a| a != "--discord-test").nth(1).and_then(|s| s.parse().ok()).unwrap_or(16);
    h.update(Some(discord::Presence {
        app_id: discord::DEFAULT_APP_ID.to_string(),
        details,
        state,
        start: Some(start),
        buttons: true,
        large_text: format!("OVERLIME Remote Play {VERSION}"),
    }));
    for i in 0..(secs / 2) {
        std::thread::sleep(Duration::from_secs(2));
        let _ = writeln!(f, "t={}s status={}", (i + 1) * 2, h.state());
    }
    h.update(None);
    std::thread::sleep(Duration::from_secs(2));
}

fn audio_test() {
    use std::io::Write;
    let out = std::env::temp_dir().join("overlime-audio-test.txt");
    let mut f = std::fs::File::create(&out).unwrap();
    let devs = audio::list_devices();
    let _ = writeln!(f, "Geräte: {:?}", devs);
    let _ = writeln!(f, "Standard: {}", audio::default_device_name());
    match audio::start_with(tone_pull(), "", 0.5) {
        Ok(a) => {
            let _ = writeln!(f, "Start ok: {}", a.shared.info.lock().unwrap().clone());
            std::thread::sleep(Duration::from_millis(1500));
            let _ = writeln!(f, "Frames gespielt: {}  Unterbrechungen: {}  Fehler: '{}'", a.shared.frames_played.load(std::sync::atomic::Ordering::Relaxed), a.shared.underruns.load(std::sync::atomic::Ordering::Relaxed), a.shared.error.lock().unwrap().clone());
        }
        Err(e) => {
            let _ = writeln!(f, "Start Fehler: {e}");
        }
    }
}

/// Fortschritt (0 bis 1, weich auslaufend) für das Einblenden eines Fensters. Startet bei jedem Öffnen neu.
fn modal_progress(ctx: &egui::Context, key: &str) -> f32 {
    let id = egui::Id::new(("modal_t0", key));
    let now = ctx.input(|i| i.time);
    let t0 = ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || now));
    let p = ((now - t0) / 0.30) as f32;
    if p < 1.0 {
        ctx.request_repaint();
    }
    ease_out_t(p.clamp(0.0, 1.0))
}

/// Merkt sich, dass das Fenster geschlossen ist, damit es beim nächsten Öffnen wieder einblendet.
fn modal_reset(ctx: &egui::Context, key: &str) {
    let id = egui::Id::new(("modal_t0", key));
    ctx.data_mut(|d| d.remove::<f64>(id));
}

fn crash_path() -> Option<std::path::PathBuf> {
    Some(std::path::PathBuf::from(std::env::var("APPDATA").ok()?).join("overlime").join("crash.txt"))
}

/// Bei einem Programmfehler eine Datei mit Fehlerbeschreibung ablegen. Beim nächsten Start wird angeboten, sie zu senden.
fn install_crash_hook() {
    std::panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let mut text = format!("OVERLIME {VERSION}\n{info}\n\n{bt}\n");
        if let Ok(home) = std::env::var("USERPROFILE") {
            text = text.replace(&home, "%USERPROFILE%");
        }
        let text = mask_ips(&text);
        if let Some(p) = crash_path() {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, text);
        }
    }));
}

fn main() -> eframe::Result<()> {
    install_crash_hook();
    lang::install();
    lang::set(Settings::load().language);
    if std::env::args().any(|a| a == "--crash-test") {
        panic!("Absturz-Test");
    }
    if let Some(i) = std::env::args().position(|a| a == "--rec-selftest") {
        // overlime --rec-selftest <eingabe.h264> <ausgabe.mp4> <hevc 0/1> <breite> <hoehe>
        let a: Vec<String> = std::env::args().collect();
        let (inp, out) = (std::ffi::CString::new(a[i + 1].clone()).unwrap(), std::ffi::CString::new(a[i + 2].clone()).unwrap());
        let mut err = [0 as std::ffi::c_char; 160];
        let r = unsafe { ffi::ol_record_selftest(inp.as_ptr(), out.as_ptr(), a[i + 3].parse().unwrap_or(0), a[i + 4].parse().unwrap_or(1280), a[i + 5].parse().unwrap_or(720), err.as_mut_ptr(), err.len()) };
        std::process::exit(r);
    }
    if std::env::args().any(|a| a == "--timing") {
        use std::io::Write;
        let mut out = String::new();
        macro_rules! tm {
            ($name:expr, $e:expr) => {{
                let t0 = Instant::now();
                let r = $e;
                out.push_str(&format!("{:<22} {:>6.1} ms
", $name, t0.elapsed().as_secs_f64() * 1000.0));
                r
            }};
        }
        ffi::init();
        let _ = tm!("Settings::load", Settings::load());
        let _ = tm!("store::load", store::load());
        let _ = tm!("local_ip", discovery::local_ip());
        let _ = tm!("autostart_enabled", tray::autostart_enabled());
        let _ = tm!("audio::list_devices", audio::list_devices());
        let _p = tm!("Pad::new (SDL)", pad::Pad::new());
        let _ = tm!("theme::all", theme::all());
        if let Ok(mut f) = std::fs::File::create(std::env::temp_dir().join("ol_timing.txt")) {
            let _ = f.write_all(out.as_bytes());
        }
        return Ok(());
    }
    if std::env::args().any(|a| a == "--settings-dump") {
        let s = Settings::load();
        let p = store::load();
        let _ = std::fs::write(std::env::temp_dir().join("ol_dump.txt"), format!("bitrate={} name={} tour={} gekoppelt={}", s.bitrate, s.psn_name, s.tour_done, p.len()));
        return Ok(());
    }
    if std::env::args().any(|a| a == "--discover") {
        let list = discovery::discover(None, Duration::from_millis(3000)).unwrap_or_default();
        let mut out = format!("lokale IP: {:?}
Gefunden: {}
", discovery::local_ip(), list.len());
        for c in list {
            out.push_str(&format!("- {} | {} | {:?} | Software {} | {}
", c.name, if c.is_ps5 { "PS5" } else { "PS4" }, match c.state { State::Ready => "bereit", State::Standby => "Ruhemodus", State::Unknown => "unbekannt" }, c.system_version, c.addr));
        }
        let _ = std::fs::write(std::env::temp_dir().join("ol_discover.txt"), out);
        return Ok(());
    }
    if std::env::args().any(|a| a == "--autostart-status") {
        std::process::exit(if tray::autostart_enabled() { 0 } else { 4 });
    }
    if std::env::args().any(|a| a == "--toast-test") {
        tray::toast("OVERLIME", "Test: PS5-Wohnzimmer ist bereit. Klick auf das Symbol zum Verbinden.");
        std::thread::sleep(std::time::Duration::from_secs(3));
        return Ok(());
    }
    if std::env::args().any(|a| a == "--tray") {
        tray::run();
        return Ok(());
    }
    if !std::env::args().any(|a| a.starts_with("--") && a != "--connect-now") {
        if !winui::single_instance() {
            return Ok(());
        }
        // Nach einem Update wieder das Symbol neben der Uhr starten (falls gewünscht)
        if tray::autostart_enabled() {
            tray::spawn_tray();
        }
    } else if std::env::args().any(|a| a == "--after-update") {
        // Neue Version nach einem Update: gleiche Aufgaben wie beim normalen Start, aber ohne Prüfung auf ein zweites Fenster
        winui::claim_instance();
        if tray::autostart_enabled() {
            tray::spawn_tray();
        }
    }
    if std::env::args().any(|a| a == "--check-test") {
        match update::check("0.30") {
            Ok(Some(r)) => println!("neu: {} {} {}", r.version, r.url, r.sig_url),
            Ok(None) => println!("aktuell"),
            Err(e) => println!("fehler: {e}"),
        }
        return Ok(());
    }
    if let Some(i) = std::env::args().position(|a| a == "--verify-sig") {
        let a: Vec<String> = std::env::args().collect();
        let data = a.get(i + 1).and_then(|p| std::fs::read(p).ok()).unwrap_or_default();
        let sig = a.get(i + 2).and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
        std::process::exit(if update::verify_signature(&data, &sig).is_ok() { 0 } else { 3 });
    }
    ffi::init();
    if std::env::args().any(|a| a == "--audio-test") {
        audio_test();
        return Ok(());
    }
    if std::env::args().any(|a| a == "--discord-test") {
        discord_test();
        return Ok(());
    }
    if std::env::args().any(|a| a == "--selftest") {
        selftest();
        return Ok(());
    }
    let mut vb = egui::ViewportBuilder::default()
        .with_inner_size([960.0, 620.0])
        .with_min_inner_size([780.0, 600.0])
        .with_title("OVERLIME Remote Play")
        .with_icon(std::sync::Arc::new(winui::app_icon()));
    // Nach einem Update: Fenster an derselben Stelle und in derselben Größe wie vorher
    if std::env::args().any(|a| a == "--after-update") {
        let pair = |key: &str| -> Option<(f32, f32)> {
            let v = std::env::args().find_map(|a| a.strip_prefix(key).map(|v| v.to_string()))?;
            let (a, b) = v.split_once(',')?;
            Some((a.parse().ok()?, b.parse().ok()?))
        };
        if std::env::args().any(|a| a == "--maximized") {
            vb = vb.with_maximized(true);
        } else {
            if let Some((w, h)) = pair("--size=") {
                vb = vb.with_inner_size([w.max(780.0), h.max(600.0)]);
            }
            if let Some((x, y)) = pair("--pos=") {
                vb = vb.with_position([x, y]);
            }
        }
    }
    let opts = eframe::NativeOptions { viewport: vb, ..Default::default() };
    eframe::run_native("OVERLIME Remote Play", opts, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
