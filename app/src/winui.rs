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

/// App-Symbol: dunkle, abgerundete Kachel mit leuchtendem Ring und Play-Dreieck.
pub fn app_icon() -> IconData {
    let n = 128usize;
    let mut rgba = vec![0u8; n * n * 4];
    let c = (n as f32 - 1.0) / 2.0;
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32 - c, y as f32 - c);
            // abgerundetes Quadrat (Superellipse)
            let k = (fx.abs() / 60.0).powf(5.0) + (fy.abs() / 60.0).powf(5.0);
            let mut a = (1.0 - k).clamp(0.0, 1.0);
            a = (a * 6.0).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            // Hintergrundverlauf
            let t = (y as f32 / n as f32 + x as f32 / n as f32) / 2.0;
            let mut r = 14.0 + 18.0 * t;
            let mut g = 22.0 + 30.0 * t;
            let mut b = 52.0 + 60.0 * t;
            // Ring
            let d = (fx * fx + fy * fy).sqrt();
            let ring = (1.0 - ((d - 40.0).abs() / 7.0)).clamp(0.0, 1.0);
            let ang = fy.atan2(fx);
            let sweep = if ang > -2.2 && ang < 2.6 { 1.0 } else { 0.35 };
            let w = ring * sweep;
            r = r * (1.0 - w) + 80.0 * w;
            g = g * (1.0 - w) + 165.0 * w;
            b = b * (1.0 - w) + 255.0 * w;
            // Play-Dreieck
            let px = fx + 6.0;
            let inside = px > -14.0 && px < 20.0 && fy.abs() < (20.0 - px) * 0.9 + 0.0 && fy.abs() < 22.0 - (px + 14.0) * 0.64;
            if inside {
                r = 240.0;
                g = 246.0;
                b = 255.0;
            }
            let i = (y * n + x) * 4;
            rgba[i] = r as u8;
            rgba[i + 1] = g as u8;
            rgba[i + 2] = b as u8;
            rgba[i + 3] = (a * 255.0) as u8;
        }
    }
    IconData { rgba, width: n as u32, height: n as u32 }
}
