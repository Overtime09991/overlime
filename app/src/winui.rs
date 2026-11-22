//! Windows-Feinheiten: Fensterleiste in Theme-Farbe (Windows 11) und App-Symbol.
use eframe::egui::{Color32, IconData};
use std::ffi::c_void;

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;
    #[link(name = "dwmapi")]
    extern "system" {
        pub fn DwmSetWindowAttribute(hwnd: isize, attr: u32, val: *const c_void, size: u32) -> i32;
    }
    #[link(name = "user32")]
    extern "system" {
        pub fn FindWindowW(class: *const u16, title: *const u16) -> isize;
    }
}

fn colorref(c: Color32) -> u32 {
    (c.r() as u32) | ((c.g() as u32) << 8) | ((c.b() as u32) << 16)
}

/// Färbt die Fensterleiste (Windows 11). Auf älteren Systemen passiert nichts.
pub fn style_titlebar(title: &str, caption: Color32, border: Color32) {
    #[cfg(windows)]
    unsafe {
        let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let hwnd = sys::FindWindowW(std::ptr::null(), wide.as_ptr());
        if hwnd == 0 {
            return;
        }
        let set = |attr: u32, v: u32| {
            let _ = sys::DwmSetWindowAttribute(hwnd, attr, &v as *const u32 as *const c_void, 4);
        };
        set(20, 1); // dunkler Modus
        set(35, colorref(caption)); // Leistenfarbe
        set(36, colorref(Color32::from_rgb(235, 240, 255))); // Titeltext
        set(34, 0xFFFF_FFFE); // kein Rahmen (DWMWA_COLOR_NONE)
        let _ = border;
    }
    #[cfg(not(windows))]
    let _ = (title, caption, border);
}

/// App-Symbol: kräftige, abgerundete Kachel mit Farbverlauf und großem weißem Play-Dreieck (auch klein gut lesbar).
pub fn app_icon() -> IconData {
    let n = 256usize;
    let mut rgba = vec![0u8; n * n * 4];
    let c = (n as f32 - 1.0) / 2.0;
    // Abstand eines Punktes zu einem Dreieck (0 im Inneren)
    fn seg(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
        let (abx, aby) = (b.0 - a.0, b.1 - a.1);
        let t = (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / (abx * abx + aby * aby)).clamp(0.0, 1.0);
        ((p.0 - a.0 - abx * t).powi(2) + (p.1 - a.1 - aby * t).powi(2)).sqrt()
    }
    let tri = [(-34.0f32, -58.0f32), (-34.0, 58.0), (62.0, 0.0)];
    let dist_tri = |p: (f32, f32)| -> f32 {
        let s = |a: (f32, f32), b: (f32, f32)| (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
        let (d1, d2, d3) = (s(tri[0], tri[1]), s(tri[1], tri[2]), s(tri[2], tri[0]));
        if (d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0) || (d1 <= 0.0 && d2 <= 0.0 && d3 <= 0.0) {
            0.0
        } else {
            seg(p, tri[0], tri[1]).min(seg(p, tri[1], tri[2])).min(seg(p, tri[2], tri[0]))
        }
    };
    for y in 0..n {
        for x in 0..n {
            let mut acc = [0.0f32; 4];
            // 3x3 Unterabtastung für glatte Kanten
            for sy in 0..3 {
                for sx in 0..3 {
                    let fx = x as f32 + (sx as f32 + 0.5) / 3.0 - 0.5 - c;
                    let fy = y as f32 + (sy as f32 + 0.5) / 3.0 - 0.5 - c;
                    let k = (fx.abs() / 122.0).powf(5.0) + (fy.abs() / 122.0).powf(5.0);
                    let a = ((1.0 - k) * 14.0).clamp(0.0, 1.0);
                    if a <= 0.0 {
                        continue;
                    }
                    // Farbverlauf: helles Blau links oben, Violett rechts unten
                    let t = ((fx + fy) / (2.0 * 244.0) + 0.5).clamp(0.0, 1.0);
                    let mut r = 44.0 + (128.0 - 44.0) * t;
                    let mut g = 132.0 + (84.0 - 132.0) * t;
                    let mut b = 255.0 + (240.0 - 255.0) * t;
                    // sanfter Lichtschein oben
                    let hl = (1.0 - (fy + 122.0) / 150.0).clamp(0.0, 1.0) * 0.16;
                    r += (255.0 - r) * hl;
                    g += (255.0 - g) * hl;
                    b += (255.0 - b) * hl;
                    // Dreieck mit gerundeten Ecken (leicht nach rechts verschoben)
                    let d = dist_tri((fx - 6.0, fy));
                    let w = (1.0 - (d - 14.0)).clamp(0.0, 1.0);
                    if w > 0.0 {
                        r += (255.0 - r) * w;
                        g += (255.0 - g) * w;
                        b += (255.0 - b) * w;
                    }
                    acc[0] += r * a;
                    acc[1] += g * a;
                    acc[2] += b * a;
                    acc[3] += a;
                }
            }
            if acc[3] > 0.0 {
                let i = (y * n + x) * 4;
                rgba[i] = (acc[0] / acc[3]) as u8;
                rgba[i + 1] = (acc[1] / acc[3]) as u8;
                rgba[i + 2] = (acc[2] / acc[3]) as u8;
                rgba[i + 3] = (acc[3] / 9.0 * 255.0) as u8;
            }
        }
    }
    IconData { rgba, width: n as u32, height: n as u32 }
}

#[cfg(windows)]
mod sys2 {
    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateMutexW(attr: *const std::ffi::c_void, owner: i32, name: *const u16) -> isize;
        pub fn GetLastError() -> u32;
        pub fn GlobalLock(h: isize) -> *const u16;
        pub fn GlobalUnlock(h: isize) -> i32;
    }
    #[link(name = "user32")]
    extern "system" {
        pub fn FindWindowW(class: *const u16, title: *const u16) -> isize;
        pub fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
        pub fn SetForegroundWindow(hwnd: isize) -> i32;
        pub fn OpenClipboard(hwnd: isize) -> i32;
        pub fn CloseClipboard() -> i32;
        pub fn GetClipboardData(fmt: u32) -> isize;
    }
}

/// Nur ein OVERLIME-Fenster gleichzeitig: Läuft schon eins, wird es nach vorn geholt und `false` zurückgegeben.
pub fn single_instance() -> bool {
    #[cfg(windows)]
    unsafe {
        let name: Vec<u16> = "Local\\OVERLIME_MAIN".encode_utf16().chain(std::iter::once(0)).collect();
        let _h = sys2::CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if sys2::GetLastError() == 183 {
            let t: Vec<u16> = "OVERLIME Remote Play".encode_utf16().chain(std::iter::once(0)).collect();
            let hwnd = sys2::FindWindowW(std::ptr::null(), t.as_ptr());
            if hwnd != 0 {
                sys2::ShowWindow(hwnd, 9);
                sys2::SetForegroundWindow(hwnd);
            }
            return false;
        }
    }
    true
}

/// Text aus der Windows-Zwischenablage (leer, wenn keiner da ist).
pub fn clipboard_text() -> String {
    #[cfg(windows)]
    unsafe {
        if sys2::OpenClipboard(0) == 0 {
            return String::new();
        }
        let mut out = String::new();
        let h = sys2::GetClipboardData(13);
        if h != 0 {
            let p = sys2::GlobalLock(h);
            if !p.is_null() {
                let mut n = 0;
                while *p.add(n) != 0 && n < 8192 {
                    n += 1;
                }
                out = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
                sys2::GlobalUnlock(h);
            }
        }
        sys2::CloseClipboard();
        return out;
    }
    #[allow(unreachable_code)]
    String::new()
}

#[repr(C)]
struct SysTime {
    year: u16,
    month: u16,
    dow: u16,
    day: u16,
    hour: u16,
    min: u16,
    sec: u16,
    ms: u16,
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetLocalTime(t: *mut SysTime);
}

/// Aktuelle Uhrzeit als "HH:MM".
pub fn local_time_hm() -> String {
    #[cfg(windows)]
    unsafe {
        let mut t = SysTime { year: 0, month: 0, dow: 0, day: 0, hour: 0, min: 0, sec: 0, ms: 0 };
        GetLocalTime(&mut t);
        return format!("{:02}:{:02}", t.hour, t.min);
    }
    #[allow(unreachable_code)]
    String::new()
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn EnumDisplaySettingsW(device: *const u16, mode: i32, dm: *mut u8) -> i32;
}

/// Bildwiederholrate des Hauptbildschirms in Hz (0 = unbekannt).
pub fn display_refresh_hz() -> u32 {
    #[cfg(windows)]
    unsafe {
        let mut dm = [0u8; 220];
        dm[68] = 220; // dmSize
        if EnumDisplaySettingsW(std::ptr::null(), -1, dm.as_mut_ptr()) != 0 {
            return u32::from_le_bytes([dm[184], dm[185], dm[186], dm[187]]);
        }
    }
    0
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

/// Ist die Windows-Sprache Deutsch?
pub fn system_is_german() -> bool {
    #[cfg(windows)]
    unsafe {
        return GetUserDefaultUILanguage() & 0x3ff == 0x07;
    }
    #[allow(unreachable_code)]
    true
}

/// Legt die Instanz-Sperre an (ohne ein vorhandenes Fenster nach vorn zu holen). Für den Übergang nach einem Update.
pub fn claim_instance() {
    #[cfg(windows)]
    unsafe {
        let name: Vec<u16> = "Local\\OVERLIME_MAIN".encode_utf16().chain(std::iter::once(0)).collect();
        let _h = sys2::CreateMutexW(std::ptr::null(), 0, name.as_ptr());
    }
}
