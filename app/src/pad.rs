//! Controller-Eingabe über SDL2 (DualSense/DualShock/Xbox/andere, auch per Bluetooth).
//! Minimale eigene FFI-Anbindung, SDL2 wird statisch eingebunden.
use std::ffi::{c_char, c_int, c_void, CStr, CString};

extern "C" {
    fn SDL_SetHint(name: *const c_char, value: *const c_char) -> c_int;
    fn SDL_Init(flags: u32) -> c_int;
    fn SDL_PollEvent(ev: *mut u64) -> c_int;
    fn SDL_NumJoysticks() -> c_int;
    fn SDL_IsGameController(idx: c_int) -> c_int;
    fn SDL_GameControllerOpen(idx: c_int) -> *mut c_void;
    fn SDL_GameControllerClose(c: *mut c_void);
    fn SDL_GameControllerGetAxis(c: *mut c_void, axis: c_int) -> i16;
    fn SDL_GameControllerGetButton(c: *mut c_void, button: c_int) -> u8;
    fn SDL_GameControllerRumble(c: *mut c_void, low: u16, high: u16, ms: u32) -> c_int;
    fn SDL_GameControllerName(c: *mut c_void) -> *const c_char;
    fn SDL_GameControllerGetJoystick(c: *mut c_void) -> *mut c_void;
    fn SDL_JoystickInstanceID(j: *mut c_void) -> i32;
    fn SDL_JoystickNameForIndex(idx: c_int) -> *const c_char;
    fn SDL_JoystickGetDeviceVendor(idx: c_int) -> u16;
    fn SDL_JoystickGetDeviceProduct(idx: c_int) -> u16;
    fn SDL_GameControllerGetType(c: *mut c_void) -> c_int;
    fn SDL_GameControllerGetNumTouchpads(c: *mut c_void) -> c_int;
    fn SDL_GameControllerGetTouchpadFinger(c: *mut c_void, touchpad: c_int, finger: c_int, state: *mut u8, x: *mut f32, y: *mut f32, pressure: *mut f32) -> c_int;
    fn SDL_GameControllerHasSensor(c: *mut c_void, t: c_int) -> c_int;
    fn SDL_GameControllerSetSensorEnabled(c: *mut c_void, t: c_int, enabled: c_int) -> c_int;
    fn SDL_GameControllerGetSensorData(c: *mut c_void, t: c_int, data: *mut f32, n: c_int) -> c_int;
    fn SDL_GameControllerHasLED(c: *mut c_void) -> c_int;
    fn SDL_GameControllerSetLED(c: *mut c_void, r: u8, g: u8, b: u8) -> c_int;
    fn SDL_GameControllerSetPlayerIndex(c: *mut c_void, index: c_int);
    fn SDL_GameControllerSendEffect(c: *mut c_void, data: *const c_void, size: c_int) -> c_int;
    fn SDL_GameControllerGetFirmwareVersion(c: *mut c_void) -> u16;
    fn SDL_JoystickCurrentPowerLevel(j: *mut c_void) -> c_int;
    fn SDL_JoystickOpen(idx: c_int) -> *mut c_void;
    fn SDL_JoystickClose(j: *mut c_void);
    fn SDL_JoystickNumAxes(j: *mut c_void) -> c_int;
    fn SDL_JoystickNumButtons(j: *mut c_void) -> c_int;
    fn SDL_JoystickNumHats(j: *mut c_void) -> c_int;
    fn SDL_JoystickGetAxis(j: *mut c_void, axis: c_int) -> i16;
    fn SDL_JoystickGetButton(j: *mut c_void, button: c_int) -> u8;
    fn SDL_JoystickGetHat(j: *mut c_void, hat: c_int) -> u8;
    fn SDL_JoystickUpdate();
    fn SDL_JoystickGetDeviceGUID(idx: c_int) -> SdlGuid;
    fn SDL_JoystickGetGUIDString(g: SdlGuid, buf: *mut c_char, len: c_int);
    fn SDL_GameControllerAddMapping(s: *const c_char) -> c_int;
    fn SDL_RWFromFile(file: *const c_char, mode: *const c_char) -> *mut c_void;
    fn SDL_GameControllerAddMappingsFromRW(rw: *mut c_void, freerw: c_int) -> c_int;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SdlGuid {
    data: [u8; 16],
}

const EV_JOY_ADDED: u32 = 0x605;

fn db_path() -> Option<std::path::PathBuf> {
    Some(std::path::PathBuf::from(std::env::var("APPDATA").ok()?).join("overlime").join("gamecontrollerdb.txt"))
}

fn guid_string(idx: c_int) -> String {
    let mut buf = [0 as c_char; 64];
    unsafe {
        let g = SDL_JoystickGetDeviceGUID(idx);
        SDL_JoystickGetGUIDString(g, buf.as_mut_ptr(), 64);
        CStr::from_ptr(buf.as_ptr()).to_string_lossy().to_string()
    }
}

fn device_name(idx: c_int) -> String {
    unsafe {
        let np = SDL_JoystickNameForIndex(idx);
        if np.is_null() { "Controller".to_string() } else { CStr::from_ptr(np).to_string_lossy().replace(',', " ") }
    }
}

/// Eigene Zuordnungen laden (Datei im OVERLIME-Ordner) und unbekannte Controller mit einer allgemeinen Belegung versehen.
fn setup_mappings() {
    unsafe {
        if let Some(path) = db_path() {
            if path.exists() {
                if let (Ok(f), Ok(m)) = (CString::new(path.to_string_lossy().as_bytes()), CString::new("rb")) {
                    let rw = SDL_RWFromFile(f.as_ptr(), m.as_ptr());
                    if !rw.is_null() {
                        SDL_GameControllerAddMappingsFromRW(rw, 1);
                    }
                }
            }
        }
        for i in 0..SDL_NumJoysticks() {
            automap(i);
        }
    }
}

/// Allgemeine Belegung für unbekannte Controller (Nachbauten). Passt oft, sonst hilft „Controller einrichten“.
fn automap(idx: c_int) -> bool {
    unsafe {
        if SDL_IsGameController(idx) != 0 {
            return false;
        }
        let j = SDL_JoystickOpen(idx);
        if j.is_null() {
            return false;
        }
        let (axes, buttons, hats) = (SDL_JoystickNumAxes(j), SDL_JoystickNumButtons(j), SDL_JoystickNumHats(j));
        SDL_JoystickClose(j);
        if axes < 2 || buttons < 6 {
            return false;
        }
        let pad_part = if hats >= 1 { "dpup:h0.1,dpright:h0.2,dpdown:h0.4,dpleft:h0.8" } else { "dpup:b12,dpdown:b13,dpleft:b14,dpright:b15" };
        let body = if axes >= 6 {
            "a:b0,b:b1,x:b2,y:b3,back:b6,start:b7,leftshoulder:b4,rightshoulder:b5,leftstick:b8,rightstick:b9,leftx:a0,lefty:a1,rightx:a2,righty:a3,lefttrigger:a4,righttrigger:a5"
        } else {
            "a:b1,b:b2,x:b0,y:b3,leftshoulder:b4,rightshoulder:b5,lefttrigger:b6,righttrigger:b7,back:b8,start:b9,leftstick:b10,rightstick:b11,guide:b12,leftx:a0,lefty:a1,rightx:a2,righty:a3"
        };
        let line = format!("{},{} (allgemein),{},{},platform:Windows", guid_string(idx), device_name(idx), body, pad_part);
        match CString::new(line) {
            Ok(c) => SDL_GameControllerAddMapping(c.as_ptr()) >= 0,
            Err(_) => false,
        }
    }
}

pub const WIZ_STEPS: &[(&str, &str)] = &[
    ("a", "Kreuz (untere Taste)"),
    ("b", "Kreis (rechte Taste)"),
    ("x", "Quadrat (linke Taste)"),
    ("y", "Dreieck (obere Taste)"),
    ("leftshoulder", "L1 (linke Schultertaste)"),
    ("rightshoulder", "R1 (rechte Schultertaste)"),
    ("lefttrigger", "L2 (linken Trigger ganz durchdrücken)"),
    ("righttrigger", "R2 (rechten Trigger ganz durchdrücken)"),
    ("back", "Share / Create / Select"),
    ("start", "Options / Start"),
    ("leftstick", "L3 (linken Stick eindrücken)"),
    ("rightstick", "R3 (rechten Stick eindrücken)"),
    ("dpup", "Steuerkreuz nach oben"),
    ("dpdown", "Steuerkreuz nach unten"),
    ("dpleft", "Steuerkreuz nach links"),
    ("dpright", "Steuerkreuz nach rechts"),
    ("leftx", "Linken Stick ganz nach rechts"),
    ("lefty", "Linken Stick ganz nach unten"),
    ("rightx", "Rechten Stick ganz nach rechts"),
    ("righty", "Rechten Stick ganz nach unten"),
    ("guide", "PS- oder Home-Taste (kann übersprungen werden)"),
];

/// Assistent: der Nutzer drückt nacheinander die Tasten, daraus entsteht eine eigene Belegung.
pub struct Wizard {
    joy: *mut c_void,
    pub name: String,
    guid: String,
    pub step: usize,
    base_axes: Vec<i16>,
    map: Vec<(String, String)>,
    wait_idle: bool,
    pub finished: bool,
}

impl Drop for Wizard {
    fn drop(&mut self) {
        if !self.joy.is_null() {
            unsafe { SDL_JoystickClose(self.joy) };
        }
    }
}

impl Wizard {
    pub fn prompt(&self) -> &'static str {
        WIZ_STEPS.get(self.step).map(|s| s.1).unwrap_or("")
    }

    fn is_axis_step(&self) -> bool {
        matches!(WIZ_STEPS.get(self.step).map(|s| s.0), Some("leftx" | "lefty" | "rightx" | "righty" | "lefttrigger" | "righttrigger"))
    }

    pub fn skip(&mut self) {
        self.next();
    }

    fn next(&mut self) {
        self.step += 1;
        self.wait_idle = true;
        if self.step >= WIZ_STEPS.len() {
            self.finished = true;
        }
    }

    /// Einmal pro Bild aufrufen.
    pub fn poll(&mut self) {
        if self.finished || self.joy.is_null() {
            return;
        }
        unsafe {
            SDL_JoystickUpdate();
            let j = self.joy;
            let (na, nb, nh) = (SDL_JoystickNumAxes(j), SDL_JoystickNumButtons(j), SDL_JoystickNumHats(j));
            if self.base_axes.len() != na as usize {
                self.base_axes = (0..na).map(|a| SDL_JoystickGetAxis(j, a)).collect();
            }
            // Erst warten, bis alles wieder in Ruhe ist
            let any_button = (0..nb).any(|b| SDL_JoystickGetButton(j, b) != 0);
            let any_hat = (0..nh).any(|h| SDL_JoystickGetHat(j, h) != 0);
            let moved: Vec<(c_int, i32)> = (0..na).filter_map(|a| {
                let d = SDL_JoystickGetAxis(j, a) as i32 - self.base_axes[a as usize] as i32;
                if d.abs() > 16000 { Some((a, d)) } else { None }
            }).collect();
            if self.wait_idle {
                if !any_button && !any_hat && moved.is_empty() {
                    self.wait_idle = false;
                }
                return;
            }
            let key = WIZ_STEPS[self.step].0.to_string();
            let axis_step = self.is_axis_step();
            let mut found: Option<String> = None;
            if axis_step {
                if let Some((a, d)) = moved.first() {
                    let trig = key.ends_with("trigger");
                    found = Some(if !trig && *d < 0 { format!("a{}~", a) } else { format!("a{}", a) });
                }
            } else {
                if let Some(b) = (0..nb).find(|b| SDL_JoystickGetButton(j, *b) != 0) {
                    found = Some(format!("b{}", b));
                } else if let Some((h, v)) = (0..nh).map(|h| (h, SDL_JoystickGetHat(j, h))).find(|(_, v)| *v != 0) {
                    found = Some(format!("h{}.{}", h, v));
                }
            }
            if let Some(v) = found {
                self.map.push((key, v));
                self.next();
            }
        }
    }

    /// Fertige Zuordnung speichern und sofort aktivieren.
    pub fn finish(&self) -> Result<(), String> {
        if self.map.len() < 8 {
            return Err("Zu wenige Tasten erkannt. Bitte noch einmal versuchen.".into());
        }
        let body: Vec<String> = self.map.iter().map(|(k, v)| format!("{k}:{v}")).collect();
        let line = format!("{},{},{},platform:Windows", self.guid, self.name, body.join(","));
        let c = CString::new(line.clone()).map_err(|e| e.to_string())?;
        if unsafe { SDL_GameControllerAddMapping(c.as_ptr()) } < 0 {
            return Err("Die Belegung wurde nicht angenommen.".into());
        }
        if let Some(path) = db_path() {
            if let Some(d) = path.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            // alte Zeile desselben Controllers ersetzen
            let old = std::fs::read_to_string(&path).unwrap_or_default();
            let mut keep: Vec<&str> = old.lines().filter(|l| !l.starts_with(&format!("{},", self.guid))).collect();
            keep.push(&line);
            let _ = std::fs::write(&path, keep.join("\n") + "\n");
        }
        Ok(())
    }
}

/// Zusatzdaten für DualSense: Touchpad und Bewegung.
#[derive(Clone, Copy, Default)]
pub struct Extra {
    pub touch_down: [bool; 2],
    pub touch_x: [f32; 2],
    pub touch_y: [f32; 2],
    pub has_motion: bool,
    pub gyro: [f32; 3],
    pub accel: [f32; 3],
}

const SDL_INIT_JOYSTICK: u32 = 0x0000_0200;
const SDL_INIT_GAMECONTROLLER: u32 = 0x0000_2000;
const SDL_INIT_EVENTS: u32 = 0x0000_4000;
const EV_BUTTON_DOWN: u32 = 0x651;
const EV_ADDED: u32 = 0x653;
const EV_REMOVED: u32 = 0x654;

// SDL_GameControllerButton
const B_A: c_int = 0;
const B_B: c_int = 1;
const B_X: c_int = 2;
const B_Y: c_int = 3;
const B_BACK: c_int = 4;
const B_GUIDE: c_int = 5;
const B_START: c_int = 6;
const B_LSTICK: c_int = 7;
const B_RSTICK: c_int = 8;
const B_LSHOULDER: c_int = 9;
const B_RSHOULDER: c_int = 10;
const B_UP: c_int = 11;
const B_DOWN: c_int = 12;
const B_LEFT: c_int = 13;
const B_RIGHT: c_int = 14;
const B_TOUCHPAD: c_int = 20;

#[derive(Clone, Copy)]
pub struct Input {
    pub buttons: u32,
    pub l2: u8,
    pub r2: u8,
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
}

pub struct Pad {
    ok: bool,
    ctrl: *mut c_void,
    instance: i32,
    /// Alle geöffneten Controller (Instanz-Nummer, Zeiger). `ctrl` ist davon der gerade benutzte.
    all: Vec<(i32, *mut c_void)>,
    pub ds_intensity: u8,
    init_rx: Option<std::sync::mpsc::Receiver<bool>>,
}

fn deadzone(v: i16) -> i16 {
    if (v as i32).abs() < 2600 {
        0
    } else {
        v
    }
}

impl Pad {
    /// Startet SDL im Hintergrund (das dauert ca. 0,2 s), damit das Fenster sofort erscheint.
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut ok = false;
            unsafe {
                let hints = [
                    ("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1"),
                    ("SDL_JOYSTICK_HIDAPI_PS5", "1"),
                    ("SDL_JOYSTICK_HIDAPI_PS4", "1"),
                    ("SDL_JOYSTICK_HIDAPI_PS5_RUMBLE", "1"),
                    ("SDL_JOYSTICK_HIDAPI_PS4_RUMBLE", "1"),
                ];
                for (k, v) in hints {
                    let (k, v) = (CString::new(k).unwrap(), CString::new(v).unwrap());
                    SDL_SetHint(k.as_ptr(), v.as_ptr());
                }
                if SDL_Init(SDL_INIT_JOYSTICK | SDL_INIT_GAMECONTROLLER | SDL_INIT_EVENTS) == 0 {
                    ok = true;
                    setup_mappings();
                }
            }
            let _ = tx.send(ok);
        });
        Self { ok: false, ctrl: std::ptr::null_mut(), instance: -1, all: Vec::new(), ds_intensity: 0, init_rx: Some(rx) }
    }

    /// Prüft, ob der Hintergrundstart fertig ist.
    fn ensure_ready(&mut self) {
        let Some(rx) = &self.init_rx else { return };
        match rx.try_recv() {
            Ok(ok) => {
                self.init_rx = None;
                self.ok = ok;
                if ok {
                    self.open_any();
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(_) => self.init_rx = None,
        }
    }

    /// Öffnet alle angeschlossenen Controller. Der erste wird benutzt, bis auf einem anderen eine Taste gedrückt wird.
    fn open_any(&mut self) {
        if !self.ok {
            return;
        }
        unsafe {
            for i in 0..SDL_NumJoysticks() {
                if SDL_IsGameController(i) != 0 {
                    self.open(i);
                }
            }
        }
    }

    fn open(&mut self, idx: c_int) {
        unsafe {
            let c = SDL_GameControllerOpen(idx);
            if c.is_null() {
                return;
            }
            let id = SDL_JoystickInstanceID(SDL_GameControllerGetJoystick(c));
            if self.all.iter().any(|(i, _)| *i == id) {
                // schon offen: den zusätzlichen Zähler von SDL wieder freigeben
                SDL_GameControllerClose(c);
                return;
            }
            // Bewegungssensoren (Beschleunigung = 1, Gyro = 2), falls vorhanden
            for t in [1, 2] {
                if SDL_GameControllerHasSensor(c, t) != 0 {
                    SDL_GameControllerSetSensorEnabled(c, t, 1);
                }
            }
            self.all.push((id, c));
            if self.ctrl.is_null() {
                self.ctrl = c;
                self.instance = id;
            }
        }
    }

    /// Wechselt zu dem Controller mit dieser Instanz-Nummer.
    fn use_instance(&mut self, id: i32) {
        if let Some((i, c)) = self.all.iter().find(|(i, _)| *i == id).copied() {
            self.ctrl = c;
            self.instance = i;
        }
    }

    fn close_all(&mut self) {
        for (_, c) in self.all.drain(..) {
            unsafe { SDL_GameControllerClose(c) };
        }
        self.ctrl = std::ptr::null_mut();
        self.instance = -1;
    }

    fn pump(&mut self) {
        self.ensure_ready();
        if !self.ok {
            return;
        }
        let mut ev = [0u64; 8];
        unsafe {
            while SDL_PollEvent(ev.as_mut_ptr()) != 0 {
                // Aufbau eines SDL-Ereignisses: Art (4 Byte), Zeitstempel (4 Byte), dann die Geräte-Nummer
                let ty = (ev[0] & 0xffff_ffff) as u32;
                let which = (ev[1] & 0xffff_ffff) as u32 as i32;
                if ty == EV_JOY_ADDED {
                    if automap(which) {
                        self.open(which);
                    }
                } else if ty == EV_ADDED {
                    self.open(which);
                } else if ty == EV_REMOVED {
                    if let Some(pos) = self.all.iter().position(|(i, _)| *i == which) {
                        let (_, c) = self.all.remove(pos);
                        SDL_GameControllerClose(c);
                        if self.instance == which {
                            self.ctrl = std::ptr::null_mut();
                            self.instance = -1;
                            if let Some(next) = self.all.first().map(|(i, _)| *i) {
                                self.use_instance(next);
                            }
                        }
                    }
                } else if ty == EV_BUTTON_DOWN && which != self.instance {
                    self.use_instance(which);
                }
            }
        }
    }

    pub fn connected(&mut self) -> bool {
        self.pump();
        !self.ctrl.is_null()
    }

    pub fn name(&self) -> String {
        if self.ctrl.is_null() {
            return String::new();
        }
        unsafe {
            let p = SDL_GameControllerName(self.ctrl);
            if p.is_null() {
                String::new()
            } else {
                CStr::from_ptr(p).to_string_lossy().to_string()
            }
        }
    }

    /// Vibration (0-255 je Motor) vom Spiel an den Controller.
    pub fn rumble(&mut self, left: u8, right: u8) {
        if self.ctrl.is_null() {
            return;
        }
        if self.is_dualsense() {
            let i = self.ds_intensity;
            self.rumble_dualsense(left, right, i);
            return;
        }
        unsafe {
            SDL_GameControllerRumble(self.ctrl, left as u16 * 257, right as u16 * 257, 250);
        }
    }

    pub fn poll(&mut self) -> Input {
        self.pump();
        let mut inp = Input { buttons: 0, l2: 0, r2: 0, lx: 0, ly: 0, rx: 0, ry: 0 };
        if self.ctrl.is_null() {
            return inp;
        }
        unsafe {
            let c = self.ctrl;
            let map: [(c_int, u32); 15] = [
                (B_A, 1 << 0),
                (B_B, 1 << 1),
                (B_X, 1 << 2),
                (B_Y, 1 << 3),
                (B_LEFT, 1 << 4),
                (B_RIGHT, 1 << 5),
                (B_UP, 1 << 6),
                (B_DOWN, 1 << 7),
                (B_LSHOULDER, 1 << 8),
                (B_RSHOULDER, 1 << 9),
                (B_LSTICK, 1 << 10),
                (B_RSTICK, 1 << 11),
                (B_START, 1 << 12),
                (B_BACK, 1 << 13),
                (B_GUIDE, 1 << 15),
            ];
            for (b, mask) in map {
                if SDL_GameControllerGetButton(c, b) != 0 {
                    inp.buttons |= mask;
                }
            }
            if SDL_GameControllerGetButton(c, B_TOUCHPAD) != 0 {
                inp.buttons |= 1 << 14;
            }
            inp.lx = deadzone(SDL_GameControllerGetAxis(c, 0));
            inp.ly = deadzone(SDL_GameControllerGetAxis(c, 1));
            inp.rx = deadzone(SDL_GameControllerGetAxis(c, 2));
            inp.ry = deadzone(SDL_GameControllerGetAxis(c, 3));
            let tl = SDL_GameControllerGetAxis(c, 4).max(0) as i32;
            let tr = SDL_GameControllerGetAxis(c, 5).max(0) as i32;
            inp.l2 = (tl * 255 / 32767) as u8;
            inp.r2 = (tr * 255 / 32767) as u8;
        }
        inp
    }

    /// Ist es ein PS5-Controller (DualSense)?
    pub fn is_dualsense(&self) -> bool {
        !self.ctrl.is_null() && unsafe { SDL_GameControllerGetType(self.ctrl) } == 7
    }

    /// Akkustand als Text, falls bekannt.
    pub fn battery(&self) -> Option<&'static str> {
        if self.ctrl.is_null() {
            return None;
        }
        unsafe {
            match SDL_JoystickCurrentPowerLevel(SDL_GameControllerGetJoystick(self.ctrl)) {
                0 => Some("leer"),
                1 => Some("niedrig"),
                2 => Some("mittel"),
                3 => Some("voll"),
                4 => Some("Kabel"),
                _ => None,
            }
        }
    }

    /// Touchpad und Bewegungssensoren auslesen.
    pub fn poll_extra(&mut self) -> Extra {
        let mut e = Extra::default();
        if self.ctrl.is_null() {
            return e;
        }
        unsafe {
            let c = self.ctrl;
            if SDL_GameControllerGetNumTouchpads(c) > 0 {
                for f in 0..2usize {
                    let (mut st, mut x, mut y, mut pr) = (0u8, 0f32, 0f32, 0f32);
                    if SDL_GameControllerGetTouchpadFinger(c, 0, f as c_int, &mut st, &mut x, &mut y, &mut pr) == 0 && st != 0 {
                        e.touch_down[f] = true;
                        e.touch_x[f] = x;
                        e.touch_y[f] = y;
                    }
                }
            }
            if SDL_GameControllerHasSensor(c, 1) != 0 && SDL_GameControllerHasSensor(c, 2) != 0 {
                let mut a = [0f32; 3];
                let mut g = [0f32; 3];
                if SDL_GameControllerGetSensorData(c, 1, a.as_mut_ptr(), 3) == 0 && SDL_GameControllerGetSensorData(c, 2, g.as_mut_ptr(), 3) == 0 {
                    e.has_motion = true;
                    e.gyro = g;
                    e.accel = [a[0] / 9.80665, a[1] / 9.80665, a[2] / 9.80665];
                }
            }
        }
        e
    }

    fn send_ds5(&self, fx: &[u8; 47]) {
        if !self.ctrl.is_null() {
            unsafe {
                SDL_GameControllerSendEffect(self.ctrl, fx.as_ptr() as *const c_void, 47);
            }
        }
    }

    /// DualSense-Vibration (links/rechts, 0-255) mit der Stärke, die das Spiel vorgibt.
    pub fn rumble_dualsense(&mut self, left: u8, right: u8, intensity: u8) {
        let mut fx = [0u8; 47];
        let fw = unsafe { SDL_GameControllerGetFirmwareVersion(self.ctrl) };
        if fw != 0 && fw < 0x0224 {
            fx[0] |= 0x01;
            fx[3] = left >> 1;
            fx[2] = right >> 1;
        } else {
            fx[38] |= 0x04;
            fx[3] = left;
            fx[2] = right;
        }
        fx[36] = intensity;
        fx[1] |= 0x40;
        fx[0] |= 0x02;
        self.send_ds5(&fx);
    }

    /// Adaptive Trigger: Effekt-Typ und 10 Parameter je Seite.
    pub fn trigger_effects(&mut self, type_l: u8, data_l: &[u8; 10], type_r: u8, data_r: &[u8; 10], intensity: u8) {
        if !self.is_dualsense() {
            return;
        }
        let mut fx = [0u8; 47];
        fx[36] = intensity;
        fx[1] |= 0x40;
        fx[0] |= 0x04 | 0x08;
        fx[10] = type_r;
        fx[11..21].copy_from_slice(data_r);
        fx[21] = type_l;
        fx[22..32].copy_from_slice(data_l);
        self.send_ds5(&fx);
    }

    /// Lichtleiste des Controllers (Farbe vom Spiel).
    pub fn set_led(&mut self, r: u8, g: u8, b: u8) {
        if !self.ctrl.is_null() && unsafe { SDL_GameControllerHasLED(self.ctrl) } != 0 {
            unsafe {
                SDL_GameControllerSetLED(self.ctrl, r, g, b);
            }
        }
    }

    pub fn set_player(&mut self, idx: u8) {
        if !self.ctrl.is_null() {
            unsafe {
                SDL_GameControllerSetPlayerIndex(self.ctrl, idx as c_int);
            }
        }
    }

    /// Alle Geräte, die SDL sieht: (Name, als Gamepad nutzbar, Hersteller-ID, Produkt-ID).
    pub fn devices(&self) -> Vec<(String, bool, u16, u16)> {
        let mut v = Vec::new();
        if !self.ok {
            return v;
        }
        unsafe {
            for i in 0..SDL_NumJoysticks() {
                let np = SDL_JoystickNameForIndex(i);
                let name = if np.is_null() { "?".to_string() } else { CStr::from_ptr(np).to_string_lossy().to_string() };
                v.push((name, SDL_IsGameController(i) != 0, SDL_JoystickGetDeviceVendor(i), SDL_JoystickGetDeviceProduct(i)));
            }
        }
        v
    }

    /// Controller neu suchen (nach dem Koppeln per Bluetooth).
    pub fn rescan(&mut self) {
        self.close_all();
        setup_mappings();
        self.pump();
        self.open_any();
    }

    /// Startet den Einrichtungs-Assistenten für das Gerät mit diesem Index.
    pub fn wizard_start(&self, idx: i32) -> Option<Wizard> {
        if !self.ok {
            return None;
        }
        unsafe {
            let j = SDL_JoystickOpen(idx);
            if j.is_null() {
                return None;
            }
            Some(Wizard { joy: j, name: device_name(idx), guid: guid_string(idx), step: 0, base_axes: Vec::new(), map: Vec::new(), wait_idle: true, finished: false })
        }
    }
}
