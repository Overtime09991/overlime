//! Auto-Highlights: erkennt laute Momente im Spielton (Explosion, Jubel, Treffer) und meldet sie.
//! Läuft komplett lokal. Die Erkennung vergleicht die kurzzeitige Lautstärke mit dem langsam mitlaufenden Durchschnitt.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

pub static ENABLED: AtomicBool = AtomicBool::new(false);
pub static SENSITIVITY: AtomicU32 = AtomicU32::new(1);
static TRIGGER: AtomicBool = AtomicBool::new(false);

pub struct Detector {
    base: f32,
    fast: f32,
    loud: f32,
    cooldown: f32,
    warmup: f32,
}

impl Detector {
    pub const fn new() -> Detector {
        Detector { base: 0.03, fast: 0.0, loud: 0.0, cooldown: 0.0, warmup: 8.0 }
    }

    pub fn reset(&mut self) {
        *self = Detector::new();
    }

    /// Gibt true zurück, wenn ein Highlight erkannt wurde. `pcm` ist Stereo (interleaved) bei 48 kHz.
    pub fn feed(&mut self, pcm: &[i16], sens: u32) -> bool {
        if pcm.len() < 2 {
            return false;
        }
        let frames = (pcm.len() / 2) as f32;
        let dt = frames / 48000.0;
        let mut acc = 0f64;
        for v in pcm {
            let x = *v as f64 / 32768.0;
            acc += x * x;
        }
        let rms = (acc / pcm.len() as f64).sqrt() as f32;
        // schneller Mittelwert (ca. 0,25 s) und langsamer Hintergrundpegel (ca. 15 s)
        self.fast += (rms - self.fast) * (dt / 0.25).min(1.0);
        self.base += (rms - self.base) * (dt / 15.0).min(1.0);
        self.cooldown = (self.cooldown - dt).max(0.0);
        if self.warmup > 0.0 {
            self.warmup -= dt;
            return false;
        }
        let thr = [3.6f32, 2.8, 2.2][sens.min(2) as usize];
        let spike = self.fast > (self.base * thr).max(0.10);
        if spike {
            self.loud += dt;
        } else {
            self.loud = (self.loud - dt * 2.0).max(0.0);
        }
        if self.loud >= 0.18 && self.cooldown <= 0.0 {
            self.loud = 0.0;
            self.cooldown = 25.0;
            return true;
        }
        false
    }
}

static DET: Mutex<Detector> = Mutex::new(Detector::new());

/// Vom Ton-Ausgang aufgerufen (jedes Mal, wenn Spielton abgespielt wird).
pub fn feed(pcm: &[i16]) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(mut d) = DET.lock() {
        if d.feed(pcm, SENSITIVITY.load(Ordering::Relaxed)) {
            TRIGGER.store(true, Ordering::Relaxed);
        }
    }
}

pub fn take_trigger() -> bool {
    TRIGGER.swap(false, Ordering::Relaxed)
}

pub fn reset() {
    if let Ok(mut d) = DET.lock() {
        d.reset();
    }
    TRIGGER.store(false, Ordering::Relaxed);
}
