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
mod theme;
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
const VERSION: &str = "0.20";

#[derive(PartialEq, Clone, Copy)]
enum Page {
    Home,
    Consoles,
    Settings,
    Diagnose,
    Help,
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
    auto_wake_done: bool,
    diag_port: Option<(bool, String)>,
    diag_busy: bool,
    applied_theme: String,
    toasts: Vec<(String, u8, f64)>,
    now: f64,
    pad_name_cache: String,
    shot_requested: bool,
    feedback: Option<bool>,
    audio_devices: Vec<String>,
    audio_test: Option<(audio::Audio, f64)>,
    muted: bool,
    gl_ok: bool,
    quick: bool,
    last_connect: Option<(Console, Paired, (usize, usize, i32, usize))>,
    pending_reconnect: Option<(Console, Paired, Instant)>,
    reconnecting: bool,
    update_click: bool,
    app_start: Instant,
    check_click: bool,
    update_checking: bool,
    update_checked_at: Option<Instant>,
    last_update_check: Instant,
    demo_gpu: Option<Arc<video::VideoGpu>>,
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
            auto_wake_done: false,
            diag_port: None,
            diag_busy: false,
            applied_theme: String::new(),
            toasts: Vec::new(),
            now: 0.0,
            pad_name_cache: String::new(),
            shot_requested: false,
            feedback: None,
            audio_devices: audio::list_devices(),
            audio_test: None,
            muted: false,
            gl_ok: false,
            quick: false,
            last_connect: None,
            pending_reconnect: None,
            reconnecting: false,
            update_click: false,
            app_start: Instant::now(),
            check_click: false,
            update_checking: false,
            update_checked_at: None,
            last_update_check: Instant::now(),
            demo_gpu: None,
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
                    notes: "### Neu in 0.20\n- **Neue Funktion** zum Ausprobieren\n- Schnelleres Verbinden\n- Viele kleine Verbesserungen\n".into(),
                });
                app.update_checked = true;
            }
            if std::env::args().any(|a| a == "--demo-update") {
                app.update = Some(update::Release { version: "0.11".into(), url: String::new(), sha_url: String::new(), notes: String::new() });
                app.update_busy = true;
                let prog = app.upd_progress.clone();
                std::thread::spawn(move || {
                    let t0 = Instant::now();
                    loop {
                        let el = t0.elapsed().as_secs_f32();
                        let (stage, pct) = if el < 2.0 { (0, 0.0) } else if el < 7.0 { (1, (el - 2.0) / 5.0 * 100.0) } else if el < 10.0 { (2, 0.0) } else if el < 13.0 { (3, 0.0) } else { (4, 0.0) };
                        if let Ok(mut g) = prog.lock() {
                            g.stage = stage;
                            g.pct = pct;
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
            if std::env::args().any(|a| a == "--diag-page") {
                app.page = Page::Diagnose;
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

    fn stop_stream(&mut self, ctx: &egui::Context) {
        let info = self.session.as_ref().map(|s| (s.state, s.ended_error, s.started.elapsed().as_secs()));
        self.stream_menu = false;
        self.show_keys = false;
        self.close_hint = false;
        self.session = None; // Drop beendet die Sitzung
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
                    if self.settings.auto_update && !self.update_busy && self.session.is_none() && self.app_start.elapsed() < Duration::from_secs(90) {
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
                (format!("Spielt {game}"), format!("auf {console}"))
            } else {
                (format!("Spielt auf {console}"), "OVERLIME Remote Play".to_string())
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

fn segmented(ui: &mut egui::Ui, value: &mut usize, labels: &[&str]) {
    ui.horizontal(|ui| {
        for (i, l) in labels.iter().enumerate() {
            let sel = *value == i;
            let b = egui::Button::new(RichText::new(*l).size(14.0).color(if sel { Color32::WHITE } else { TEXT_DIM }))
                .fill(if sel { th_blue() } else { th_card_hover() })
                .rounding(10.0)
                .min_size(Vec2::new(90.0, 34.0));
            if ui.add(b).clicked() {
                *value = i;
            }
        }
    });
}

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none().fill(th_card()).rounding(16.0).inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).size(17.0).strong());
        ui.add_space(4.0);
        add(ui);
    });
}

fn label(ui: &mut egui::Ui, title: &str, desc: &str) {
    ui.label(RichText::new(title).size(14.0).strong());
    if !desc.is_empty() {
        ui.label(RichText::new(desc).size(12.5).color(TEXT_DIM));
    }
}

fn nav_item(ui: &mut egui::Ui, icon: &str, text: &str, selected: bool, badge: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), egui::Sense::click());
    let h = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    if !selected && h > 0.01 {
        ui.painter().rect_filled(rect, Rounding::same(12.0), th_card_hover().gamma_multiply(0.45 * h));
    }
    let col = if selected { Color32::WHITE } else { TEXT_DIM.lerp_to_gamma(Color32::WHITE, h) };
    let ic_col = if selected { th_blue_light() } else { col };
    ui.painter().text(rect.left_center() + Vec2::new(18.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(18.0), ic_col);
    ui.painter().text(rect.left_center() + Vec2::new(48.0, 0.0), Align2::LEFT_CENTER, text, FontId::proportional(15.0), col);
    if badge {
        let c = rect.right_center() - Vec2::new(16.0, 0.0);
        ui.painter().circle_filled(c, 9.0, th_blue());
        ui.painter().text(c, Align2::CENTER_CENTER, "!", FontId::proportional(12.0), Color32::WHITE);
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
        let st = &mut self.settings;
        egui::ScrollArea::vertical().show(ui, |ui| {
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
                ui.add_space(6.0);
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
            ui.add_space(6.0);
            section(ui, &format!("{}  Videoqualität", ic::FILM_STRIP), |ui| {
                label(ui, "Auflösung", "Sony erlaubt im Remote Play höchstens 1080p.");
                segmented(ui, &mut st.resolution, &["540p", "720p", "1080p"]);
                ui.add_space(6.0);
                label(ui, "Bildrate", "");
                segmented(ui, &mut st.fps, &["30 FPS", "60 FPS"]);
                ui.add_space(6.0);
                label(ui, "Bitrate", "Höher = schärferes Bild, braucht aber eine schnelle Leitung.");
                ui.add(egui::Slider::new(&mut st.bitrate, 2.0..=50.0).suffix(" Mbit/s"));
                ui.add_space(6.0);
                label(ui, "Videocodec", "HEVC (H.265) ist bei gleicher Bitrate schärfer. Automatisch: HEVC bei der PS5, H.264 bei der PS4.");
                segmented(ui, &mut st.codec, &["Automatisch", "H.264", "HEVC"]);
            });
            ui.add_space(6.0);
            section(ui, &format!("{}  Darstellung", ic::MONITOR), |ui| {
                label(ui, "Hochskalierung auf 4K", "Die Konsole sendet max. 1080p. Die Grafikkarte rechnet hoch: „Aus“ = harte Pixel, „Weich“ = glatt, „Scharf“ und „Ultra“ = Schärfefilter (CAS).");
                segmented(ui, &mut st.upscale, &["Aus", "Weich", "Scharf", "Ultra"]);
                ui.add_space(6.0);
                ui.checkbox(&mut st.fullscreen, "Vollbild beim Streamen");
            });
            ui.add_space(6.0);
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
            ui.add_space(6.0);
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
            ui.add_space(6.0);
            section(ui, &format!("{}  Steuerung", ic::GAME_CONTROLLER), |ui| {
                ui.checkbox(&mut st.rumble, "Controller-Vibration");
                ui.checkbox(&mut st.ds_features, "DualSense-Funktionen: Touchpad, Bewegungssensor, adaptive Trigger, Lichtleiste (gilt ab dem nächsten Stream)");
                ui.label(RichText::new("Controller (PS5, PS4, Xbox, andere) werden automatisch erkannt. Die Tastatur stellst du weiter unten ein.").color(TEXT_DIM).size(12.5));
            });
            ui.add_space(6.0);
            section(ui, &format!("{}  Tastatur", ic::KEYBOARD), |ui| {
                ui.label(RichText::new("Die Tastatur steuert das Spiel nur, wenn du sie brauchst. Die Kurztasten (Esc, F1, F10, F11, F12, M) funktionieren immer.").color(TEXT_DIM).size(12.5));
                ui.add_space(4.0);
                segmented(ui, &mut st.kb_mode, &["Automatisch (nur ohne Controller)", "Immer", "Aus"]);
                ui.add_space(6.0);
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
            ui.add_space(6.0);
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
            ui.add_space(6.0);
            section(ui, &format!("{}  Verbindung", ic::GLOBE), |ui| {
                label(ui, "Konsole per IP hinzufügen", "Nur für Internet/VPN (z. B. Tailscale). Im Heimnetz wird alles automatisch gefunden.");
                ui.add(egui::TextEdit::singleline(&mut st.manual_ip).hint_text("IP-Adresse der Konsole").desired_width(260.0));
                ui.add_space(6.0);
                ui.add_space(6.0);
                if !st.psn_account.is_empty() && ui.button("PlayStation-Konto abmelden").clicked() {
                    st.psn_account.clear();
                    st.psn_name.clear();
                }
            });
            ui.add_space(10.0);
            ui.label(RichText::new(format!("OVERLIME Remote Play {VERSION}")).color(TEXT_DIM).size(12.0));
        });
    }

    fn dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dialog.take() else { return };
        // Hintergrund abdunkeln
        egui::Area::new(egui::Id::new("dim")).order(egui::Order::Foreground).fixed_pos(Pos2::ZERO).show(ctx, |ui| {
            let r = ctx.screen_rect();
            ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_black_alpha(170));
            ui.allocate_rect(r, egui::Sense::click());
        });
        let mut close = false;
        let mut next: Option<Step> = None;
        egui::Window::new("pair")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([520.0, 0.0])
            .order(egui::Order::Tooltip)
            .frame(egui::Frame::none().fill(th_card()).rounding(20.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.5))).inner_margin(egui::Margin::same(26.0)))
            .show(ctx, |ui| {
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
            return false;
        }
        let mut apply = false;
        let running = self.last_connect.as_ref().map(|(_, _, pr)| *pr);
        let cur = (self.settings.resolution, self.settings.fps, self.settings.bitrate as i32, self.settings.codec);
        let differs = running.map(|r| r != cur).unwrap_or(false);
        egui::Window::new("quick")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::RIGHT_TOP, [-14.0, 64.0])
            .order(egui::Order::Foreground)
            .frame(egui::Frame::none().fill(Color32::from_rgba_unmultiplied(12, 16, 32, 240)).rounding(16.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.4))).inner_margin(egui::Margin::same(18.0)))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}  Schnell-Einstellungen", ic::SLIDERS)).size(16.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(egui::Button::new("X").fill(Color32::from_white_alpha(14)).rounding(8.0)).clicked() {
                            self.quick = false;
                        }
                    });
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Bildschärfe (sofort)").strong().size(13.5));
                segmented(ui, &mut self.settings.upscale, &["Aus", "Weich", "Scharf", "Ultra"]);
                ui.add_enabled_ui(self.settings.upscale >= 2, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Stärke").size(12.5).color(TEXT_DIM));
                        ui.add(egui::Slider::new(&mut self.settings.sharp, 0.0..=1.0).show_value(false));
                    });
                });
                ui.checkbox(&mut self.show_hud, "Statusanzeige oben links (F10)");
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(RichText::new("Qualität (verbindet kurz neu)").strong().size(13.5));
                ui.label(RichText::new("Die PlayStation legt Auflösung und Datenrate beim Verbinden fest.").color(TEXT_DIM).size(11.5));
                segmented(ui, &mut self.settings.resolution, &["540p", "720p", "1080p"]);
                segmented(ui, &mut self.settings.fps, &["30 FPS", "60 FPS"]);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Bitrate").size(12.5).color(TEXT_DIM));
                    ui.add(egui::Slider::new(&mut self.settings.bitrate, 2.0..=50.0).suffix(" Mbit/s"));
                });
                segmented(ui, &mut self.settings.codec, &["Auto", "H.264", "HEVC"]);
                ui.add_space(6.0);
                if differs {
                    if ui.add(big_button("Anwenden und neu verbinden", th_blue()).min_size(Vec2::new(324.0, 42.0))).clicked() {
                        apply = true;
                    }
                } else {
                    ui.label(RichText::new("Diese Qualität läuft gerade.").color(GREEN).size(12.5));
                }
            });
        apply
    }

    /// Pause-Menü und Tastenbelegung im Stream. Gibt (weiter, vollbild, tasten, stream_beenden, app_beenden) zurück.
    fn stream_overlays(&mut self, ctx: &egui::Context) -> (bool, bool, bool, bool, bool) {
        let (mut resume, mut fs, mut keys_btn, mut stop, mut quit) = (false, false, false, false, false);
        let mut shot = false;
        if self.stream_menu {
            egui::Area::new(egui::Id::new("menu_dim")).order(egui::Order::Foreground).fixed_pos(Pos2::ZERO).show(ctx, |ui| {
                let r = ctx.screen_rect();
                ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_black_alpha(150));
                ui.allocate_rect(r, egui::Sense::click());
            });
            egui::Window::new("pause")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .order(egui::Order::Tooltip)
                .frame(egui::Frame::none().fill(th_card()).rounding(20.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.5))).inner_margin(egui::Margin::same(26.0)))
                .show(ctx, |ui| {
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
                    if ui.add(big_button("Vollbild (F11)", th_card_hover()).min_size(Vec2::new(360.0, 42.0))).clicked() {
                        fs = true;
                    }
                    if ui.add(big_button("Tastenbelegung (F1)", th_card_hover()).min_size(Vec2::new(360.0, 42.0))).clicked() {
                        keys_btn = true;
                    }
                    if ui.add(big_button("Screenshot (F12)", th_card_hover()).min_size(Vec2::new(360.0, 42.0))).clicked() {
                        shot = true;
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}  Lautstärke", ic::SPEAKER_HIGH)).size(14.0));
                        ui.add(egui::Slider::new(&mut self.settings.volume, 0.0..=1.5).show_value(false));
                        ui.checkbox(&mut self.muted, "Stumm (M)");
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
                                ui.label(RichText::new(text).size(13.5));
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
        if self.settings.autoconnect && !self.auto_done && self.session.is_none() && self.starting.is_none() {
            if let Some(c) = list.iter().find(|c| c.state == State::Ready && self.paired_for(c).is_some()) {
                if let Some(p) = self.paired_for(c) {
                    self.auto_done = true;
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
        r.push_str("\n--- Protokoll (letzte Zeilen) ---\n");
        let tail: Vec<&str> = self.log.lines().rev().take(40).collect();
        for l in tail.into_iter().rev() {
            r.push_str(&mask_ips(l));
            r.push('\n');
        }
        r
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
        let Some(err) = self.feedback else { return };
        egui::Area::new(egui::Id::new("fb_dim")).order(egui::Order::Foreground).fixed_pos(Pos2::ZERO).show(ctx, |ui| {
            let r = ctx.screen_rect();
            ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_black_alpha(150));
            ui.allocate_rect(r, egui::Sense::click());
        });
        let (mut good, mut bad, mut later) = (false, false, false);
        egui::Window::new("feedback")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .order(egui::Order::Tooltip)
            .frame(egui::Frame::none().fill(th_card()).rounding(20.0).stroke(Stroke::new(1.0, th_blue().gamma_multiply(0.5))).inner_margin(egui::Margin::same(26.0)))
            .show(ctx, |ui| {
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
        if self.session.is_some() {
            self.toasts.push(("Während eines Spiels wird nichts installiert. Bitte danach aktualisieren.".to_string(), 1, self.now));
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
                let hud = format!(
                    "{}x{}  •  {} FPS{}{}{}{}",
                    s.size.0,
                    s.size.1,
                    s.fps,
                    if s.lost > 0 { format!("  •  {} Pakete verloren", s.lost) } else { String::new() },
                    if pad_connected { "  •  Controller".to_string() } else { String::new() },
                    atxt,
                    if s.ping >= 0.0 { format!("  •  Ping {:.0} ms  •  Dek. {:.1} ms  •  {:.1} Mbit/s", s.ping, s.dec_ms, s.kbps as f32 / 1000.0) } else { String::new() }
                );
                if show_hud {
                    ui.painter().text(full.left_top() + Vec2::new(14.0, 12.0), Align2::LEFT_TOP, hud, FontId::monospace(12.0), Color32::from_white_alpha(150));
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
                    if fl.mute {
                        self.muted = !self.muted;
                    }
                }
            }
        });

        if bar_shot {
            self.shot_requested = true;
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
        if apply_quality {
            if let Some((c, p, _)) = self.last_connect.clone() {
                self.quick = false;
                self.stop_stream(ctx);
                self.feedback = None;
                self.reconnecting = true;
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
    /// Controller-Bedienung der Menüs: Steuerkreuz/Stick = Auswahl, ✕ = Bestätigen, L1/R1 = Seite wechseln.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.session.is_some() {
            self.nav_prev = 0;
            return;
        }
        let inp = self.pad.poll();
        let mut b = inp.buttons;
        if inp.ly > 16000 { b |= 1 << 7; }
        if inp.ly < -16000 { b |= 1 << 6; }
        let pressed = b & !self.nav_prev;
        self.nav_prev = b;
        let key = |k: Key, shift: bool| {
            let m = egui::Modifiers { shift, ..Default::default() };
            [true, false].map(|down| egui::Event::Key { key: k, physical_key: Some(k), pressed: down, repeat: false, modifiers: m })
        };
        if pressed & (1 << 7) != 0 { raw.events.extend(key(Key::Tab, false)); }
        if pressed & (1 << 6) != 0 { raw.events.extend(key(Key::Tab, true)); }
        if pressed & (1 << 0) != 0 { raw.events.extend(key(Key::Enter, false)); }
        let pages = [Page::Home, Page::Consoles, Page::Settings, Page::Diagnose, Page::Help];
        let idx = pages.iter().position(|p| *p == self.page).unwrap_or(0);
        if pressed & (1 << 9) != 0 && self.dialog.is_none() { self.page = pages[(idx + 1) % 5]; }
        if pressed & (1 << 8) != 0 && self.dialog.is_none() { self.page = pages[(idx + 4) % 5]; }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.gl_ok = frame.gl().is_some();
        let t = ctx.input(|i| i.time);
        self.now = t;
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
            let (f5, c1, c2, c3, c4, c5) = ctx.input(|i| {
                let c = i.modifiers.ctrl;
                (
                    i.key_pressed(Key::F5),
                    c && i.key_pressed(Key::Num1),
                    c && i.key_pressed(Key::Num2),
                    c && i.key_pressed(Key::Num3),
                    c && i.key_pressed(Key::Num4),
                    c && i.key_pressed(Key::Num5),
                )
            });
            if f5 && !self.searching {
                self.search();
            }
            if c1 {
                self.page = Page::Home;
            }
            if c2 {
                self.page = Page::Consoles;
            }
            if c3 {
                self.page = Page::Settings;
            }
            if c4 {
                self.page = Page::Diagnose;
            }
            if c5 {
                self.page = Page::Help;
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
                    f.w = 1280;
                    f.h = 720;
                    f.nv12 = video::test_pattern_nv12(1280, 720);
                    f.dirty = true;
                }
                g
            }).clone();
            {
                let mut f = gpu.frame.lock().unwrap();
                f.sharp = [0.0, 0.0, 0.55, 0.9][mode.min(3)];
                f.nearest = mode == 0;
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
        // Qualität angewendet: kurz neu verbinden
        if self.reconnecting && self.session.is_none() {
            ctx.request_repaint();
            egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::BLACK)).show(ctx, |ui| {
                let r = ui.max_rect();
                let c = r.center();
                ui.painter().text(c + Vec2::new(0.0, 40.0), Align2::CENTER_CENTER, "Neue Qualität wird angewendet …", FontId::proportional(20.0), Color32::WHITE);
                ui.painter().text(c + Vec2::new(0.0, 72.0), Align2::CENTER_CENTER, "OVERLIME verbindet kurz neu", FontId::proportional(13.0), TEXT_DIM);
                ui.put(Rect::from_center_size(c - Vec2::new(0.0, 20.0), Vec2::splat(44.0)), egui::Spinner::new().size(44.0));
            });
            let go = self.pending_reconnect.as_ref().map(|(_, _, t0)| t0.elapsed() > Duration::from_millis(2500)).unwrap_or(false);
            if go {
                if let Some((c, p, _)) = self.pending_reconnect.take() {
                    self.connect(&c, &p);
                }
            }
            return;
        }
        // Update-Animation
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
            if splash::update_screen(ctx, t, prog.stage, prog.pct, self.upd_shown, VERSION, &rel.version, prog.error.as_deref()) {
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
        ctx.request_repaint_after(Duration::from_millis(16));

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
                    Page::Consoles => (ic::GAME_CONTROLLER, "Konsolen"),
                    Page::Settings => (ic::GEAR_SIX, "Einstellungen"),
                    Page::Diagnose => (ic::STETHOSCOPE, "Diagnose"),
                    Page::Help => (ic::QUESTION, "Hilfe"),
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
                let pages = [Page::Home, Page::Consoles, Page::Settings, Page::Diagnose, Page::Help];
                let items = [(ic::HOUSE, "Übersicht"), (ic::GAME_CONTROLLER, "Konsolen"), (ic::GEAR_SIX, "Einstellungen"), (ic::STETHOSCOPE, "Diagnose"), (ic::QUESTION, "Hilfe")];
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
                    if nav_item(ui, icon, label, self.page == pages[i], i == 2 && self.update.is_some()) {
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
                Page::Consoles => self.consoles_page(ui, t),
                Page::Settings => self.settings_page(ui),
                Page::Diagnose => self.diag_page(ui, t),
                Page::Help => self.help_page(ui),
            }
        });

        if self.dialog.is_none() {
            self.feedback_ui(ctx);
        }
        self.dialog_ui(ctx);
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

fn main() -> eframe::Result<()> {
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
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 620.0])
            .with_min_inner_size([780.0, 600.0])
            .with_title("OVERLIME Remote Play")
            .with_icon(std::sync::Arc::new(winui::app_icon())),
        ..Default::default()
    };
    eframe::run_native("OVERLIME Remote Play", opts, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
