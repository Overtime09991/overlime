//! Controller-Feinabstimmung: Totzone, Empfindlichkeit und Kurve der Sticks, dazu eine Zielhilfe-Verlangsamung.
//! Alles wirkt nur auf die Eingabe, die an die Konsole geht, und zielt nie von selbst.

use crate::pad::Input;
use crate::settings::Settings;

/// Kurven-Voreinstellungen: Exponent auf die Auslenkung (größer = weicher am Anfang, kleiner = direkter).
pub const CURVES: [f32; 3] = [1.7, 1.0, 0.65];

/// Wendet Totzone, Kurve und Empfindlichkeit auf einen Stick an (Auslenkung als Radius, Richtung bleibt).
pub fn stick(x: i16, y: i16, dead: f32, sens: f32, curve: f32, extra: f32) -> (i16, i16) {
    let fx = x as f32 / 32767.0;
    let fy = y as f32 / 32767.0;
    let m = (fx * fx + fy * fy).sqrt();
    if m <= 0.0001 {
        return (0, 0);
    }
    let dead = dead.clamp(0.0, 0.6);
    if m <= dead {
        return (0, 0);
    }
    // wieder auf 0..1 strecken, Kurve, Empfindlichkeit
    let mut n = ((m.min(1.0) - dead) / (1.0 - dead)).clamp(0.0, 1.0);
    n = n.powf(curve.max(0.2));
    n = (n * sens * extra).clamp(0.0, 1.0);
    let k = n / m;
    let ox = (fx * k * 32767.0).clamp(-32767.0, 32767.0) as i16;
    let oy = (fy * k * 32767.0).clamp(-32767.0, 32767.0) as i16;
    (ox, oy)
}

/// Verlangsamung des rechten Sticks beim Halten von L2 (Zielhilfe)
pub fn aim_factor(st: &Settings, l2: u8) -> f32 {
    if st.aim_slow && l2 > 40 {
        (st.aim_slow_pct / 100.0).clamp(0.1, 1.0)
    } else {
        1.0
    }
}

pub fn is_default(st: &Settings) -> bool {
    st.tune_dead_l == 0.0 && st.tune_dead_r == 0.0 && st.tune_sens_l == 1.0 && st.tune_sens_r == 1.0 && st.tune_curve_l == 1 && st.tune_curve_r == 1 && !st.aim_slow
}

/// Wendet die Einstellungen auf die Eingabe an.
pub fn apply(inp: Input, st: &Settings) -> Input {
    if is_default(st) {
        return inp;
    }
    let (lx, ly) = stick(inp.lx, inp.ly, st.tune_dead_l, st.tune_sens_l, CURVES[st.tune_curve_l.min(2)], 1.0);
    let (rx, ry) = stick(inp.rx, inp.ry, st.tune_dead_r, st.tune_sens_r, CURVES[st.tune_curve_r.min(2)], aim_factor(st, inp.l2));
    Input { lx, ly, rx, ry, ..inp }
}

pub fn reset(st: &mut Settings) {
    st.tune_dead_l = 0.0;
    st.tune_dead_r = 0.0;
    st.tune_sens_l = 1.0;
    st.tune_sens_r = 1.0;
    st.tune_curve_l = 1;
    st.tune_curve_r = 1;
    st.aim_slow = false;
    st.aim_slow_pct = 45.0;
}
