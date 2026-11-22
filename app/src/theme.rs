//! Farbschemata. Eingebaute Themes plus eigene Dateien in %APPDATA%\overlime\themes\*.txt
use eframe::egui::Color32;
use std::sync::{OnceLock, RwLock};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Theme {
    pub accent: Color32,
    pub accent_light: Color32,
    pub bg: Color32,
    pub sidebar: Color32,
    pub card: Color32,
    pub card_hover: Color32,
}

#[derive(Clone)]
pub struct Preset {
    pub name: String,
    pub theme: Theme,
    pub custom: bool,
}

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

const OZEAN: Theme = Theme { accent: c(0, 112, 224), accent_light: c(80, 165, 255), bg: c(8, 11, 22), sidebar: c(12, 17, 32), card: c(19, 26, 46), card_hover: c(28, 38, 66) };

fn cell() -> &'static RwLock<Theme> {
    static T: OnceLock<RwLock<Theme>> = OnceLock::new();
    T.get_or_init(|| RwLock::new(OZEAN))
}

pub fn set(t: Theme) {
    if let Ok(mut g) = cell().write() {
        *g = t;
    }
}

pub fn get() -> Theme {
    cell().read().map(|g| *g).unwrap_or(OZEAN)
}

pub fn th_bg() -> Color32 { get().bg }
pub fn th_sidebar() -> Color32 { get().sidebar }
pub fn th_card() -> Color32 { get().card }
pub fn th_card_hover() -> Color32 { get().card_hover }
pub fn th_blue() -> Color32 { get().accent }
pub fn th_blue_light() -> Color32 { get().accent_light }

pub fn builtin() -> Vec<Preset> {
    let p = |name: &str, theme: Theme| Preset { name: name.to_string(), theme, custom: false };
    vec![
        p("Ozean", OZEAN),
        p("Violett", Theme { accent: c(124, 58, 237), accent_light: c(167, 139, 250), bg: c(11, 8, 24), sidebar: c(16, 12, 34), card: c(26, 19, 50), card_hover: c(38, 28, 72) }),
        p("Smaragd", Theme { accent: c(5, 150, 105), accent_light: c(52, 211, 153), bg: c(6, 15, 14), sidebar: c(9, 22, 20), card: c(15, 32, 30), card_hover: c(22, 46, 43) }),
        p("Sonnenuntergang", Theme { accent: c(234, 88, 12), accent_light: c(251, 146, 60), bg: c(20, 10, 8), sidebar: c(28, 14, 11), card: c(42, 22, 17), card_hover: c(60, 32, 25) }),
        p("Mitternacht", Theme { accent: c(59, 130, 246), accent_light: c(96, 165, 250), bg: c(0, 0, 0), sidebar: c(8, 8, 10), card: c(16, 16, 20), card_hover: c(26, 26, 32) }),
    ]
}

pub fn themes_dir() -> Option<std::path::PathBuf> {
    Some(std::path::PathBuf::from(std::env::var("APPDATA").ok()?).join("overlime").join("themes"))
}

fn parse_hex(s: &str) -> Option<Color32> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// Legt eine Beispieldatei an, falls der Ordner leer ist.
pub fn ensure_example() {
    let Some(d) = themes_dir() else { return };
    let _ = std::fs::create_dir_all(&d);
    let empty = std::fs::read_dir(&d).map(|mut r| r.next().is_none()).unwrap_or(true);
    if empty {
        let _ = std::fs::write(
            d.join("Beispiel-Rosa.txt"),
            "# Eigenes Farbschema. Farben als #RRGGBB. Fehlende Zeilen nehmen die Farben von Ozean.\nname=Beispiel Rosa\naccent=#DB2777\naccent_light=#F472B6\nbg=#14080F\nsidebar=#1C0C16\ncard=#2B1220\ncard_hover=#3E1A2E\n",
        );
    }
}

/// Alle Themes: eingebaute + eigene Dateien.
pub fn all() -> Vec<Preset> {
    let mut v = builtin();
    let Some(d) = themes_dir() else { return v };
    let Ok(rd) = std::fs::read_dir(&d) else { return v };
    for e in rd.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("txt") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let mut t = OZEAN;
        let mut name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Eigenes").to_string();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((k, val)) = line.split_once('=') else { continue };
            match k.trim() {
                "name" => name = val.trim().to_string(),
                "accent" => if let Some(x) = parse_hex(val) { t.accent = x },
                "accent_light" => if let Some(x) = parse_hex(val) { t.accent_light = x },
                "bg" => if let Some(x) = parse_hex(val) { t.bg = x },
                "sidebar" => if let Some(x) = parse_hex(val) { t.sidebar = x },
                "card" => if let Some(x) = parse_hex(val) { t.card = x },
                "card_hover" => if let Some(x) = parse_hex(val) { t.card_hover = x },
                _ => {}
            }
        }
        v.push(Preset { name, theme: t, custom: true });
    }
    v
}

pub fn by_name(name: &str) -> Theme {
    all().into_iter().find(|p| p.name == name).map(|p| p.theme).unwrap_or(OZEAN)
}
