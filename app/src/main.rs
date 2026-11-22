#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod audio;
mod discovery;
mod ffi;
mod keys;
mod pad;
mod pages;
mod psn;
mod settings;
mod store;
mod update;

use discovery::{Console, State};
use eframe::egui::{
    self, epaint::RectShape, Align, Align2, Color32, FontId, Key, Layout, Pos2, Rect, RichText, Rounding, Shape, Stroke,
    TextureHandle, TextureOptions, Vec2,
};
use ffi::{Paired, Stream, StreamParams};
use settings::Settings;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

const BG: Color32 = Color32::from_rgb(8, 11, 22);
const SIDEBAR: Color32 = Color32::from_rgb(12, 17, 32);
const CARD: Color32 = Color32::from_rgb(19, 26, 46);
const CARD_HOVER: Color32 = Color32::from_rgb(28, 38, 66);
const BLUE: Color32 = Color32::from_rgb(0, 112, 224);
const BLUE_LIGHT: Color32 = Color32::from_rgb(80, 165, 255);
const GREEN: Color32 = Color32::from_rgb(52, 211, 153);
const YELLOW: Color32 = Color32::from_rgb(251, 191, 36);
const RED: Color32 = Color32::from_rgb(248, 113, 113);
const TEXT_DIM: Color32 = Color32::from_rgb(128, 141, 170);
const VERSION: &str = "0.10";

#[derive(PartialEq, Clone, Copy)]
enum Page {
    Home,
    Consoles,
    Settings,
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
    _audio: Option<audio::Audio>,
    audio_err: Option<String>,
    tex: Option<TextureHandle>,
    buf: Vec<u8>,
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
        setup_style(&cc.egui_ctx);
        let (tx, rx) = channel();
        let settings = Settings::load();
        let mut app = Self {
            page: Page::Home,
            page_since: 0.0,
            consoles: vec![],
            first_seen: HashMap::new(),
            misses: 0,
            local_ip: discovery::local_ip().map(|i| i.to_string()).unwrap_or_else(|| "?".into()),
            saved: settings.clone(),
            settings,
            paired: store::load(),
            dialog: None,
            session: None,
            starting: None,
            update: None,
            update_busy: false,
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
                if let Ok(Some(r)) = update::check(VERSION) {
                    let _ = tx.send(Msg::UpdateAvail(r));
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
        self.stream_menu = false;
        self.show_keys = false;
        self.close_hint = false;
        self.session = None; // Drop beendet die Sitzung
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
                    }
                }
                Msg::Error(e) => {
                    self.status = format!("Fehler: {}", mask_ips(&e));
                    self.searching = false;
                }
                Msg::Info(s) => self.status = s,
                Msg::PsnOk(id, name) => {
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
                    self.starting = None;
                    let stream = Arc::new(stream);
                    let (audio, audio_err) = match audio::start(stream.clone()) {
                        Ok(a) => (Some(a), None),
                        Err(e) => (None, Some(e)),
                    };
                    self.session = Some(Session {
                        stream,
                        _audio: audio,
                        audio_err,
                        tex: None,
                        buf: Vec::new(),
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
                    });
                    if self.settings.fullscreen {
                        self.fullscreen = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                    }
                }
                Msg::StreamErr(e) => {
                    self.starting = None;
                    self.start_error = Some(mask_ips(&e));
                }
                Msg::UpdateAvail(r) => {
                    if self.settings.auto_update && !self.update_busy && self.session.is_none() {
                        self.update_busy = true;
                        let tx = self.tx.clone();
                        let rr = r.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = update::install(&rr) {
                                let _ = tx.send(Msg::UpdateErr(e));
                            }
                        });
                    }
                    self.update = Some(r);
                }
                Msg::UpdateErr(e) => {
                    self.update_busy = false;
                    self.update_err = e;
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
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.override_text_color = Some(Color32::from_rgb(236, 241, 255));
    v.widgets.inactive.bg_fill = CARD_HOVER;
    v.widgets.hovered.bg_fill = CARD_HOVER;
    v.selection.bg_fill = BLUE;
    ctx.set_visuals(v);
    let mut s = (*ctx.style()).clone();
    s.spacing.item_spacing = Vec2::new(10.0, 10.0);
    s.spacing.button_padding = Vec2::new(18.0, 9.0);
    s.spacing.slider_width = 260.0;
    ctx.set_style(s);
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

/// PS5 (weiß, geschwungen) oder PS4 (schwarz, Parallelogramm), mit pulsierendem Schein.
fn draw_console(ui: &mut egui::Ui, ps5: bool, on: bool, t: f64, hover: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(130.0, 130.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    let c = rect.center();
    let pulse = if on { 0.5 + 0.5 * (t * 2.0).sin() as f32 } else { 0.0 };
    let glow = if on { BLUE } else { Color32::from_rgb(60, 70, 100) };
    p.circle_filled(c, 60.0 + 3.0 * pulse + 3.0 * hover, glow.gamma_multiply(0.10 + 0.06 * pulse));
    p.circle_filled(c, 44.0, glow.gamma_multiply(0.12));
    let led = if on { BLUE_LIGHT.gamma_multiply(0.65 + 0.35 * pulse) } else { Color32::from_rgb(90, 98, 120) };

    if ps5 {
        let w = Color32::from_rgb(242, 244, 250);
        let w2 = Color32::from_rgb(207, 212, 228);
        p.add(Shape::convex_polygon(
            vec![Pos2::new(c.x - 6.0, c.y - 52.0), Pos2::new(c.x - 28.0, c.y - 40.0), Pos2::new(c.x - 30.0, c.y + 44.0), Pos2::new(c.x - 6.0, c.y + 54.0)],
            w,
            Stroke::NONE,
        ));
        p.add(Shape::convex_polygon(
            vec![Pos2::new(c.x + 6.0, c.y - 52.0), Pos2::new(c.x + 28.0, c.y - 40.0), Pos2::new(c.x + 30.0, c.y + 44.0), Pos2::new(c.x + 6.0, c.y + 54.0)],
            w2,
            Stroke::NONE,
        ));
        p.rect_filled(Rect::from_center_size(c, Vec2::new(13.0, 104.0)), Rounding::same(6.0), Color32::from_rgb(18, 20, 28));
        p.rect_filled(Rect::from_center_size(Pos2::new(c.x, c.y - 20.0), Vec2::new(3.0, 40.0)), Rounding::same(2.0), led);
    } else {
        p.add(Shape::convex_polygon(
            vec![Pos2::new(c.x - 30.0, c.y - 40.0), Pos2::new(c.x + 52.0, c.y - 40.0), Pos2::new(c.x + 30.0, c.y + 40.0), Pos2::new(c.x - 52.0, c.y + 40.0)],
            Color32::from_rgb(28, 31, 42),
            Stroke::new(1.5, Color32::from_rgb(70, 76, 96)),
        ));
        p.add(Shape::convex_polygon(
            vec![Pos2::new(c.x - 30.0, c.y - 40.0), Pos2::new(c.x + 52.0, c.y - 40.0), Pos2::new(c.x + 46.0, c.y - 20.0), Pos2::new(c.x - 36.0, c.y - 20.0)],
            Color32::from_rgb(40, 44, 58),
            Stroke::NONE,
        ));
        p.rect_filled(Rect::from_center_size(Pos2::new(c.x - 4.0, c.y + 10.0), Vec2::new(42.0, 4.0)), Rounding::same(2.0), led);
    }
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
                .fill(if sel { BLUE } else { CARD_HOVER })
                .rounding(10.0)
                .min_size(Vec2::new(90.0, 34.0));
            if ui.add(b).clicked() {
                *value = i;
            }
        }
    });
}

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none().fill(CARD).rounding(16.0).inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
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

fn nav_item(ui: &mut egui::Ui, icon: &str, text: &str, selected: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), egui::Sense::click());
    let h = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let sel = ui.ctx().animate_bool_with_time(resp.id.with("s"), selected, 0.2);
    let bg = CARD_HOVER.gamma_multiply(0.35 * h + 0.65 * sel);
    ui.painter().rect_filled(rect, Rounding::same(12.0), bg);
    if sel > 0.01 {
        let bar = Rect::from_min_size(rect.min + Vec2::new(0.0, 10.0), Vec2::new(4.0, 24.0));
        ui.painter().rect_filled(bar, Rounding::same(2.0), BLUE_LIGHT.gamma_multiply(sel));
    }
    let col = if selected { Color32::WHITE } else { TEXT_DIM.lerp_to_gamma(Color32::WHITE, h) };
    ui.painter().text(rect.left_center() + Vec2::new(18.0, 0.0), Align2::LEFT_CENTER, format!("{icon}   {text}"), FontId::proportional(15.0), col);
    resp.clicked()
}

fn step_row(ui: &mut egui::Ui, n: &str, text: &str) {
    ui.horizontal_top(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(26.0), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), 13.0, BLUE.gamma_multiply(0.35));
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
                } else if ui.add(egui::Button::new("⟳ Neu suchen").fill(CARD).rounding(10.0)).clicked() {
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
            if list.is_empty() {
                egui::Frame::none().fill(CARD).rounding(18.0).inner_margin(egui::Margin::same(34.0)).show(ui, |ui| {
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
                                ui.label(RichText::new(if c.is_ps5 { "PlayStation 5" } else { "PlayStation 4" }).color(BLUE_LIGHT).size(13.0).strong());
                                ui.label(RichText::new(&c.name).size(24.0).strong());
                                ui.label(RichText::new(format!("Systemsoftware {}", pretty_version(&c.system_version))).color(TEXT_DIM).size(13.0));
                                ui.horizontal(|ui| {
                                    match c.state {
                                        State::Ready => pill(ui, "Bereit", GREEN),
                                        State::Standby => pill(ui, "Ruhemodus", YELLOW),
                                        State::Unknown => pill(ui, "Unbekannt", TEXT_DIM),
                                    }
                                    if paired.is_some() {
                                        pill(ui, "Gekoppelt", BLUE_LIGHT);
                                    }
                                });
                                if !c.running_app.is_empty() {
                                    ui.label(RichText::new(format!("🎮 {}", c.running_app)).color(TEXT_DIM).size(13.0));
                                }
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match &paired {
                                None => {
                                    if ui.add(big_button("Koppeln", BLUE)).clicked() {
                                        self.open_dialog(&c);
                                    }
                                }
                                Some(p) if c.state == State::Standby => {
                                    if ui.add(big_button("Aufwecken", BLUE)).clicked() {
                                        self.wake(&c, p);
                                    }
                                }
                                Some(p) => {
                                    let busy = self.starting.is_some();
                                    let txt = if busy { "Verbinde …" } else { "▶  Verbinden" };
                                    if ui.add_enabled(!busy, big_button(txt, BLUE)).clicked() {
                                        self.connect(&c, p);
                                    }
                                }
                            });
                        });
                        hv
                    });
                    let hv = inner.inner;
                    let rect = inner.response.rect;
                    let fill = CARD.lerp_to_gamma(CARD_HOVER, hv);
                    let stroke = Stroke::new(1.5, BLUE.gamma_multiply(0.7 * hv));
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
                let b = egui::Button::new(RichText::new("⚡ Maximale Qualität").strong()).fill(BLUE).rounding(10.0);
                if ui.add(b).clicked() {
                    self.settings.max_quality();
                }
            });
        });
        ui.label(RichText::new("Wird automatisch gespeichert.").color(TEXT_DIM).size(13.0));
        ui.add_space(8.0);
        let st = &mut self.settings;
        egui::ScrollArea::vertical().show(ui, |ui| {
            section(ui, "🎞  Videoqualität", |ui| {
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
            section(ui, "🖥  Darstellung", |ui| {
                label(ui, "Hochskalierung auf 4K", "Die Konsole sendet max. 1080p. „Aus“ zeigt harte Pixel, „Weich“ glättet.");
                segmented(ui, &mut st.upscale, &["Aus", "Weich", "Scharf", "Ultra"]);
                ui.add_space(6.0);
                ui.checkbox(&mut st.fullscreen, "Vollbild beim Streamen");
            });
            ui.add_space(6.0);
            section(ui, "🎮  Steuerung", |ui| {
                ui.checkbox(&mut st.rumble, "Controller-Vibration");
                ui.label(RichText::new("Controller (PS5, PS4, Xbox, andere) werden automatisch erkannt. Die Tastatur stellst du weiter unten ein.").color(TEXT_DIM).size(12.5));
            });
            ui.add_space(6.0);
            section(ui, "⌨  Tastatur", |ui| {
                ui.label(RichText::new("Alle Tasten sind frei belegbar: Auf eine Taste klicken, dann die neue Taste drücken (Esc bricht ab).").color(TEXT_DIM).size(12.5));
                ui.add_space(4.0);
                egui::Grid::new("kbgrid").num_columns(2).spacing([20.0, 6.0]).show(ui, |ui| {
                    for (i, act) in keys::ACTIONS.iter().enumerate() {
                        ui.label(act.label);
                        let cap = self.capturing == Some(i);
                        let text = if cap { "Drücke eine Taste …".to_string() } else { keys::pretty(&st.keys[i]) };
                        let b = egui::Button::new(RichText::new(text).monospace())
                            .fill(if cap { BLUE } else { CARD_HOVER })
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
                if ui.add(egui::Button::new("Auf Standard zurücksetzen").fill(CARD_HOVER).rounding(10.0)).clicked() {
                    st.keys = keys::defaults();
                    self.capturing = None;
                    self.key_note.clear();
                }
            });
            ui.add_space(6.0);
            section(ui, "🌐  Verbindung", |ui| {
                label(ui, "Konsole per IP hinzufügen", "Nur für Internet/VPN (z. B. Tailscale). Im Heimnetz wird alles automatisch gefunden.");
                ui.add(egui::TextEdit::singleline(&mut st.manual_ip).hint_text("IP-Adresse der Konsole").desired_width(260.0));
                ui.add_space(6.0);
                ui.checkbox(&mut st.auto_update, "Updates automatisch installieren");
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
            .frame(egui::Frame::none().fill(CARD).rounding(20.0).stroke(Stroke::new(1.0, BLUE.gamma_multiply(0.5))).inner_margin(egui::Margin::same(26.0)))
            .show(ctx, |ui| {
                ui.set_width(468.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} koppeln", d.console.name)).size(22.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if d.step != Step::Busy && ui.add(egui::Button::new("X").fill(CARD_HOVER).rounding(8.0)).clicked() {
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
                        if ui.add(big_button("Bei PlayStation anmelden", CARD_HOVER).min_size(Vec2::new(468.0, 44.0))).clicked() {
                            ctx.open_url(egui::OpenUrl::new_tab(psn::login_url()));
                        }
                        ui.add(egui::TextEdit::singleline(&mut d.pasted).hint_text("Adresse hier einfügen (…redirect?code=…)").desired_width(468.0));
                        if !d.message.is_empty() {
                            ui.label(RichText::new(&d.message).color(RED));
                        }
                        let code = psn::parse_code(&d.pasted);
                        if ui.add_enabled(code.is_some(), big_button("Weiter", BLUE).min_size(Vec2::new(468.0, 46.0))).clicked() {
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
                        if ui.add_enabled(ok, big_button("Koppeln", BLUE).min_size(Vec2::new(468.0, 46.0))).clicked() {
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
                        if ui.add(big_button("Fertig", BLUE).min_size(Vec2::new(468.0, 46.0))).clicked() {
                            close = true;
                        }
                    }
                    Step::Failed => {
                        ui.label(RichText::new("Kopplung fehlgeschlagen").size(18.0).strong().color(RED));
                        ui.label(RichText::new(&d.message).color(TEXT_DIM));
                        ui.add_space(4.0);
                        ui.label(RichText::new("Tipp: Der Code gilt nur kurz. Öffne „Gerät koppeln“ an der Konsole neu und gib den neuen Code ein.").color(TEXT_DIM).size(13.0));
                        ui.add_space(8.0);
                        if ui.add(big_button("Nochmal versuchen", BLUE).min_size(Vec2::new(468.0, 46.0))).clicked() {
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

    /// Pause-Menü und Tastenbelegung im Stream. Gibt (weiter, vollbild, tasten, stream_beenden, app_beenden) zurück.
    fn stream_overlays(&mut self, ctx: &egui::Context) -> (bool, bool, bool, bool, bool) {
        let (mut resume, mut fs, mut keys_btn, mut stop, mut quit) = (false, false, false, false, false);
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
                .frame(egui::Frame::none().fill(CARD).rounding(20.0).stroke(Stroke::new(1.0, BLUE.gamma_multiply(0.5))).inner_margin(egui::Margin::same(26.0)))
                .show(ctx, |ui| {
                    ui.set_width(360.0);
                    ui.label(RichText::new("Pause-Menü").size(22.0).strong());
                    if self.close_hint {
                        ui.label(RichText::new("Der Stream läuft noch. Willst du wirklich schließen?").color(YELLOW).size(13.5));
                    } else {
                        ui.label(RichText::new("Dein Spiel läuft auf der Konsole weiter.").color(TEXT_DIM).size(13.0));
                    }
                    ui.add_space(8.0);
                    if ui.add(big_button("Weiter (Esc)", BLUE).min_size(Vec2::new(360.0, 46.0))).clicked() {
                        resume = true;
                    }
                    if ui.add(big_button("Vollbild (F11)", CARD_HOVER).min_size(Vec2::new(360.0, 42.0))).clicked() {
                        fs = true;
                    }
                    if ui.add(big_button("Tastenbelegung (F1)", CARD_HOVER).min_size(Vec2::new(360.0, 42.0))).clicked() {
                        keys_btn = true;
                    }
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
                            ui.label(RichText::new(keys::pretty(&self.settings.keys[n])).monospace().color(BLUE_LIGHT).size(12.0));
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

    /// Tastatur nur auswerten, wenn das Fenster aktiv ist und kein Menü offen ist.
    fn keyboard_input(&self, ctx: &egui::Context) -> pad::Input {
        let neutral = pad::Input { buttons: 0, l2: 0, r2: 0, lx: 0, ly: 0, rx: 0, ry: 0 };
        if self.stream_menu || self.show_keys || ctx.wants_keyboard_input() || !ctx.input(|i| i.focused) {
            return neutral;
        }
        ctx.input(|i| keys::eval(i, &self.settings.keys))
    }

    fn stream_ui(&mut self, ctx: &egui::Context) {
        let mut stop = false;
        let mut open_menu = false;
        let show_hud = self.show_hud;
        let mut toggle_fs = false;
        let mut copy_log = false;
        let opts = match self.settings.upscale {
            0 => TextureOptions::NEAREST,
            _ => TextureOptions::LINEAR,
        };
        // Eingabe sammeln (Controller + Tastatur)
        let pad_in = self.pad.poll();
        let kb = self.keyboard_input(ctx);
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

        let Some(s) = self.session.as_mut() else { return };
        s.stream.set_input(input.buttons, input.l2, input.r2, input.lx, input.ly, input.rx, input.ry);
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
        if let Some((fw, fh)) = s.stream.take_frame(&mut s.buf) {
            let img = egui::ColorImage::from_rgba_unmultiplied([fw, fh], &s.buf[..fw * fh * 4]);
            match s.tex.as_mut() {
                Some(t) => t.set(img, opts),
                None => s.tex = Some(ctx.load_texture("video", img, opts)),
            }
            s.fps_count += 1;
        }
        if s.fps_t.elapsed() >= Duration::from_secs(1) {
            s.fps = s.fps_count;
            s.fps_count = 0;
            s.fps_t = Instant::now();
        }

        let mouse_moved = ctx.input(|i| i.pointer.delta() != Vec2::ZERO);
        if mouse_moved {
            s.last_input = Instant::now();
        }
        let show_ui = s.last_input.elapsed() < Duration::from_secs(3) || s.state != 1;

        egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::BLACK)).show(ctx, |ui| {
            let full = ui.max_rect();
            if let Some(tex) = &s.tex {
                let (iw, ih) = (tex.size()[0] as f32, tex.size()[1] as f32);
                let scale = (full.width() / iw).min(full.height() / ih);
                let r = Rect::from_center_size(full.center(), Vec2::new(iw * scale, ih * scale));
                ui.painter().image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }

            // Verbindungs-/Fehler-Overlay
            if s.state == 0 || s.state == 2 || s.tex.is_none() {
                let ended = s.state == 2;
                egui::Area::new(egui::Id::new("overlay")).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    egui::Frame::none().fill(CARD).rounding(20.0).stroke(Stroke::new(1.0, if ended && s.ended_error { RED.gamma_multiply(0.6) } else { BLUE.gamma_multiply(0.5) })).inner_margin(egui::Margin::same(28.0)).show(ui, |ui| {
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
                                    egui::Frame::none().fill(BG).rounding(10.0).inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(RichText::new(&log_tail).monospace().size(11.0).color(TEXT_DIM));
                                    });
                                    if ui.button("Protokoll kopieren").clicked() {
                                        copy_log = true;
                                    }
                                }
                                ui.add_space(8.0);
                                if ui.add(big_button("Zurück", BLUE).min_size(Vec2::new(380.0, 46.0))).clicked() {
                                    stop = true;
                                }
                            } else {
                                ui.add(egui::Spinner::new().size(44.0));
                                ui.add_space(8.0);
                                ui.label(RichText::new(format!("Verbinde mit {} …", s.name)).size(20.0).strong());
                                let secs = s.started.elapsed().as_secs();
                                let sub = if s.tex.is_none() && s.state == 1 { "Warte auf das erste Bild" } else { "Handshake mit der Konsole" };
                                ui.label(RichText::new(format!("{sub}  •  {secs}s")).color(TEXT_DIM));
                                if let Some(a) = &s.audio_err {
                                    ui.label(RichText::new(format!("Ton nicht verfügbar: {a}")).color(YELLOW).size(12.0));
                                }
                                ui.add_space(8.0);
                                if ui.add(big_button("Abbrechen", CARD_HOVER).min_size(Vec2::new(380.0, 44.0))).clicked() {
                                    stop = true;
                                }
                            }
                        });
                    });
                });
            }

            // HUD + Steuerleiste
            if s.state == 1 && s.tex.is_some() {
                let hud = format!(
                    "{}x{}  •  {} FPS{}{}",
                    s.size.0,
                    s.size.1,
                    s.fps,
                    if s.lost > 0 { format!("  •  {} Pakete verloren", s.lost) } else { String::new() },
                    if pad_connected { "  •  🎮" } else { "" }
                );
                if show_hud {
                    ui.painter().text(full.left_top() + Vec2::new(14.0, 12.0), Align2::LEFT_TOP, hud, FontId::monospace(12.0), Color32::from_white_alpha(150));
                }
                if show_ui {
                    egui::Area::new(egui::Id::new("bar")).order(egui::Order::Foreground).anchor(Align2::RIGHT_TOP, [-14.0, 10.0]).show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            if ui.add(egui::Button::new("Vollbild (F11)").fill(CARD).rounding(10.0)).clicked() {
                                toggle_fs = true;
                            }
                            if ui.add(egui::Button::new("Menü (Esc)").fill(CARD).rounding(10.0)).clicked() {
                                open_menu = true;
                            }
                        });
                    });
                }
            }
        });

        if ctx.input(|i| i.key_pressed(Key::F11)) {
            toggle_fs = true;
        }
        let (esc, f1, f10) = ctx.input(|i| (i.key_pressed(Key::Escape), i.key_pressed(Key::F1), i.key_pressed(Key::F10)));
        let state_now = self.session.as_ref().map(|s| s.state).unwrap_or(0);
        if esc {
            if self.show_keys {
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
        if copy_log {
            ctx.copy_text(self.log.clone());
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
        let pages = [Page::Home, Page::Consoles, Page::Settings, Page::Help];
        let idx = pages.iter().position(|p| *p == self.page).unwrap_or(0);
        if pressed & (1 << 9) != 0 && self.dialog.is_none() { self.page = pages[(idx + 1) % 4]; }
        if pressed & (1 << 8) != 0 && self.dialog.is_none() { self.page = pages[(idx + 3) % 4]; }
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let t = ctx.input(|i| i.time);
        // Nie aus Versehen mitten im Stream schließen (X-Knopf, Alt+F4)
        if self.session.is_some() && !self.allow_close && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.stream_menu = true;
            self.close_hint = true;
        }
        self.handle_messages(ctx);
        self.pump_log();
        self.process_capture(ctx, t);
        if self.session.is_none() && self.dialog.is_none() && self.capturing.is_none() && !ctx.wants_keyboard_input() {
            let (f5, c1, c2, c3, c4) = ctx.input(|i| {
                let c = i.modifiers.ctrl;
                (
                    i.key_pressed(Key::F5),
                    c && i.key_pressed(Key::Num1),
                    c && i.key_pressed(Key::Num2),
                    c && i.key_pressed(Key::Num3),
                    c && i.key_pressed(Key::Num4),
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
                self.page = Page::Help;
            }
        }

        if self.settings != self.saved {
            self.settings.save();
            self.saved = self.settings.clone();
        }

        if self.session.is_some() {
            ctx.request_repaint();
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
            p.rect_filled(r, Rounding::ZERO, BG);
            let a = (t * 0.25) as f32;
            p.circle_filled(Pos2::new(r.width() * (0.75 + 0.08 * a.sin()), r.height() * (0.2 + 0.06 * (a * 1.3).cos())), 260.0, BLUE.gamma_multiply(0.07));
            p.circle_filled(Pos2::new(r.width() * (0.35 + 0.07 * (a * 0.8).cos()), r.height() * (0.85 + 0.05 * a.sin())), 300.0, Color32::from_rgb(90, 60, 220).gamma_multiply(0.05));
        }

        egui::SidePanel::left("nav")
            .exact_width(210.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(SIDEBAR).inner_margin(egui::Margin::same(16.0)))
            .show(ctx, |ui| {
                ui.add_space(6.0);
                let (lr, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.0), egui::Sense::hover());
                {
                    let p = ui.painter_at(lr);
                    p.rect_filled(Rect::from_min_size(lr.min + Vec2::new(0.0, 6.0), Vec2::new(5.0, 44.0)), Rounding::same(3.0), BLUE_LIGHT);
                    p.text(lr.min + Vec2::new(16.0, 4.0), Align2::LEFT_TOP, "OVERLIME", FontId::proportional(27.0), Color32::WHITE);
                    p.text(lr.min + Vec2::new(17.0, 36.0), Align2::LEFT_TOP, "REMOTE PLAY", FontId::proportional(12.0), BLUE_LIGHT);
                }
                ui.add_space(18.0);
                let old = self.page;
                if nav_item(ui, "🏠", "Übersicht", self.page == Page::Home) {
                    self.page = Page::Home;
                }
                if nav_item(ui, "🎮", "Konsolen", self.page == Page::Consoles) {
                    self.page = Page::Consoles;
                }
                if nav_item(ui, "⚙", "Einstellungen", self.page == Page::Settings) {
                    self.page = Page::Settings;
                }
                if nav_item(ui, "❓", "Hilfe", self.page == Page::Help) {
                    self.page = Page::Help;
                }
                if old != self.page {
                    self.page_since = t;
                }
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.label(RichText::new(format!("Version {VERSION}")).color(TEXT_DIM).size(12.0));
                    if psn::id_from_b64(&self.settings.psn_account).is_some() {
                        let who = if self.settings.psn_name.is_empty() { "PlayStation-Konto".to_string() } else { self.settings.psn_name.clone() };
                        ui.label(RichText::new(format!("Angemeldet: {who}")).color(GREEN).size(12.0));
                    } else {
                        ui.label(RichText::new("Nicht angemeldet").color(YELLOW).size(12.0));
                    }
                    ui.label(RichText::new(if self.local_ip == "?" { "Netzwerk: offline" } else { "Netzwerk: verbunden" }).color(TEXT_DIM).size(12.0));
                    let q = format!("{} • {} FPS", ["540p", "720p", "1080p"][self.settings.resolution], [30, 60][self.settings.fps]);
                    ui.label(RichText::new(q).color(BLUE_LIGHT).size(12.0));
                });
            });

        egui::CentralPanel::default().frame(egui::Frame::none().inner_margin(egui::Margin::same(28.0))).show(ctx, |ui| {
            ui.set_opacity(fade(t, self.page_since, 0.3));
            if let Some(rel) = self.update.clone() {
                egui::Frame::none().fill(BLUE.gamma_multiply(0.22)).rounding(14.0).stroke(Stroke::new(1.0, BLUE.gamma_multiply(0.6))).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
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
                            } else if ui.add(big_button("Jetzt aktualisieren", BLUE).min_size(Vec2::new(170.0, 38.0))).clicked() {
                                self.update_busy = true;
                                self.update_err.clear();
                                let tx = self.tx.clone();
                                let r = rel.clone();
                                std::thread::spawn(move || {
                                    if let Err(e) = update::install(&r) {
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
                Page::Help => self.help_page(ui),
            }
        });

        self.dialog_ui(ctx);
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
    let params = StreamParams { host: "127.0.0.1".into(), paired, width: 1920, height: 1080, fps: 60, bitrate_kbps: 30000, hevc: true, hw_decode: true };
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

fn main() -> eframe::Result<()> {
    ffi::init();
    if std::env::args().any(|a| a == "--selftest") {
        selftest();
        return Ok(());
    }
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 620.0])
            .with_min_inner_size([760.0, 480.0])
            .with_title("OVERLIME Remote Play"),
        ..Default::default()
    };
    eframe::run_native("OVERLIME Remote Play", opts, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
