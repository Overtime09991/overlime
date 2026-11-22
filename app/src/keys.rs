//! Frei belegbare Tastatur-Steuerung (Tasten werden als egui-Namen gespeichert).
use crate::pad::Input;
use eframe::egui::{self, Key};

#[derive(Clone, Copy)]
pub enum Kind {
    Btn(u32),
    L2,
    R2,
    Lx(i16),
    Ly(i16),
    Rx(i16),
    Ry(i16),
}

pub struct Action {
    pub label: &'static str,
    pub default: &'static str,
    pub kind: Kind,
}

const fn a(label: &'static str, default: &'static str, kind: Kind) -> Action {
    Action { label, default, kind }
}

pub const ACTIONS: &[Action] = &[
    a("Linker Stick hoch", "W", Kind::Ly(-1)),
    a("Linker Stick runter", "S", Kind::Ly(1)),
    a("Linker Stick links", "A", Kind::Lx(-1)),
    a("Linker Stick rechts", "D", Kind::Lx(1)),
    a("Rechter Stick hoch", "I", Kind::Ry(-1)),
    a("Rechter Stick runter", "K", Kind::Ry(1)),
    a("Rechter Stick links", "J", Kind::Rx(-1)),
    a("Rechter Stick rechts", "L", Kind::Rx(1)),
    a("Steuerkreuz hoch", "ArrowUp", Kind::Btn(1 << 6)),
    a("Steuerkreuz runter", "ArrowDown", Kind::Btn(1 << 7)),
    a("Steuerkreuz links", "ArrowLeft", Kind::Btn(1 << 4)),
    a("Steuerkreuz rechts", "ArrowRight", Kind::Btn(1 << 5)),
    a("Kreuz", "Space", Kind::Btn(1 << 0)),
    a("Kreis", "E", Kind::Btn(1 << 1)),
    a("Quadrat", "Q", Kind::Btn(1 << 2)),
    a("Dreieck", "R", Kind::Btn(1 << 3)),
    a("L1", "Z", Kind::Btn(1 << 8)),
    a("R1", "X", Kind::Btn(1 << 9)),
    a("L2", "Shift", Kind::L2),
    a("R2", "Ctrl", Kind::R2),
    a("L3 (Stick drücken)", "C", Kind::Btn(1 << 10)),
    a("R3 (Stick drücken)", "V", Kind::Btn(1 << 11)),
    a("Options", "Tab", Kind::Btn(1 << 12)),
    a("Share / Create", "Backspace", Kind::Btn(1 << 13)),
    a("Touchpad-Taste", "T", Kind::Btn(1 << 14)),
    a("PS-Taste", "F2", Kind::Btn(1 << 15)),
];

pub fn defaults() -> Vec<String> {
    ACTIONS.iter().map(|x| x.default.to_string()).collect()
}

pub fn is_down(i: &egui::InputState, name: &str) -> bool {
    match name {
        "" => false,
        "Shift" => i.modifiers.shift,
        "Ctrl" => i.modifiers.ctrl,
        "Alt" => i.modifiers.alt,
        n => Key::from_name(n).map(|k| i.key_down(k)).unwrap_or(false),
    }
}

/// Liest die Tastatur anhand der Belegung und erzeugt einen Controller-Zustand.
pub fn eval(i: &egui::InputState, binds: &[String]) -> Input {
    let mut inp = Input { buttons: 0, l2: 0, r2: 0, lx: 0, ly: 0, rx: 0, ry: 0 };
    let (mut lx, mut ly, mut rx, mut ry) = (0i32, 0i32, 0i32, 0i32);
    for (idx, act) in ACTIONS.iter().enumerate() {
        let name = binds.get(idx).map(|s| s.as_str()).unwrap_or(act.default);
        if !is_down(i, name) {
            continue;
        }
        match act.kind {
            Kind::Btn(m) => inp.buttons |= m,
            Kind::L2 => inp.l2 = 255,
            Kind::R2 => inp.r2 = 255,
            Kind::Lx(s) => lx += s as i32 * 32000,
            Kind::Ly(s) => ly += s as i32 * 32000,
            Kind::Rx(s) => rx += s as i32 * 32000,
            Kind::Ry(s) => ry += s as i32 * 32000,
        }
    }
    inp.lx = lx.clamp(-32767, 32767) as i16;
    inp.ly = ly.clamp(-32767, 32767) as i16;
    inp.rx = rx.clamp(-32767, 32767) as i16;
    inp.ry = ry.clamp(-32767, 32767) as i16;
    inp
}

/// Lesbarer deutscher Name einer Taste.
pub fn pretty(name: &str) -> String {
    match name {
        "" => "—".into(),
        "Space" => "Leertaste".into(),
        "Backspace" => "Rücktaste".into(),
        "ArrowUp" => "Pfeil hoch".into(),
        "ArrowDown" => "Pfeil runter".into(),
        "ArrowLeft" => "Pfeil links".into(),
        "ArrowRight" => "Pfeil rechts".into(),
        "Ctrl" => "Strg".into(),
        "Escape" => "Esc".into(),
        "Enter" => "Eingabe".into(),
        "Delete" => "Entf".into(),
        "PageUp" => "Bild hoch".into(),
        "PageDown" => "Bild runter".into(),
        n => n.to_string(),
    }
}
