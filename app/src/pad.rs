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
    pub ds_intensity: u8,
}

fn deadzone(v: i16) -> i16 {
    if (v as i32).abs() < 2600 {
        0
    } else {
        v
    }
}

impl Pad {
    pub fn new() -> Self {
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
            }
        }
        let mut p = Self { ok, ctrl: std::ptr::null_mut(), instance: -1, ds_intensity: 0 };
        p.open_any();
        p
    }

    fn open_any(&mut self) {
        if !self.ok || !self.ctrl.is_null() {
            return;
        }
        unsafe {
            for i in 0..SDL_NumJoysticks() {
                if SDL_IsGameController(i) != 0 {
                    self.open(i);
                    if !self.ctrl.is_null() {
                        return;
                    }
                }
            }
        }
    }

    fn open(&mut self, idx: c_int) {
        unsafe {
            let c = SDL_GameControllerOpen(idx);
            if !c.is_null() {
                self.ctrl = c;
                self.instance = SDL_JoystickInstanceID(SDL_GameControllerGetJoystick(c));
                // Bewegungssensoren (Beschleunigung = 1, Gyro = 2), falls vorhanden
                for t in [1, 2] {
                    if SDL_GameControllerHasSensor(c, t) != 0 {
                        SDL_GameControllerSetSensorEnabled(c, t, 1);
                    }
                }
            }
        }
    }

    fn pump(&mut self) {
        if !self.ok {
            return;
        }
        let mut ev = [0u64; 8];
        unsafe {
            while SDL_PollEvent(ev.as_mut_ptr()) != 0 {
                let ty = (ev[0] & 0xffff_ffff) as u32;
                let which = (ev[0] >> 32) as u32 as i32;
                if ty == EV_ADDED && self.ctrl.is_null() {
                    self.open(which);
                } else if ty == EV_REMOVED && which == self.instance {
                    SDL_GameControllerClose(self.ctrl);
                    self.ctrl = std::ptr::null_mut();
                    self.instance = -1;
                    self.open_any();
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
}
