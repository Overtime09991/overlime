//! Video-Wiedergabe für die Galerie: Bild und Ton aus MP4-Dateien, mit Pause, Vorspulen und Lautstärke.
//! Das Entschlüsseln übernimmt ffmpeg (über die C-Schicht), der Ton läuft über dieselbe Ausgabe wie der Stream.

use std::collections::VecDeque;
use std::ffi::{c_char, c_int, CString};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::audio;

#[repr(C)]
pub struct OlPlayer {
    _p: [u8; 0],
}

extern "C" {
    fn ol_play_open(path: *const c_char, max_w: c_int, want: c_int, err: *mut c_char, err_size: usize) -> *mut OlPlayer;
    fn ol_play_info(p: *mut OlPlayer, w: *mut c_int, h: *mut c_int, duration_ms: *mut i64, has_audio: *mut c_int);
    fn ol_play_step(p: *mut OlPlayer, rgba: *mut u8, rgba_cap: usize, vpts_ms: *mut i64, aud: *mut i16, aud_cap: usize, aud_n: *mut usize, apts_ms: *mut i64) -> c_int;
    fn ol_play_seek(p: *mut OlPlayer, ms: i64) -> c_int;
    fn ol_play_close(p: *mut OlPlayer);
    fn ol_trim(inp: *const c_char, out: *const c_char, start_ms: i64, end_ms: i64, err: *mut c_char, err_size: usize) -> c_int;
    fn ol_play_thumb(path: *const c_char, max_w: c_int, rgba: *mut u8, cap: usize, w: *mut c_int, h: *mut c_int, duration_ms: *mut i64) -> c_int;
}

/// Vorschaubild einer Videodatei (RGBA) und ihre Länge.
pub struct Thumb {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
    pub duration_ms: i64,
}

/// Schneidet einen Bereich aus einer Videodatei, ohne neu zu kodieren. Der Anfang rutscht auf das davorliegende Schlüsselbild.
pub fn trim(path_in: &std::path::Path, path_out: &std::path::Path, start_ms: i64, end_ms: i64) -> Result<(), String> {
    let a = CString::new(path_in.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let b = CString::new(path_out.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let mut err = [0 as c_char; 160];
    let r = unsafe { ol_trim(a.as_ptr(), b.as_ptr(), start_ms, end_ms, err.as_mut_ptr(), err.len()) };
    if r == 0 { Ok(()) } else { Err(crate::ffi::cstr(&err)) }
}

pub fn thumbnail(path: &std::path::Path, max_w: i32) -> Option<Thumb> {
    let c = CString::new(path.to_string_lossy().as_bytes()).ok()?;
    let cap = (max_w as usize) * (max_w as usize) * 4;
    let mut buf = vec![0u8; cap];
    let (mut w, mut h, mut dur) = (0 as c_int, 0 as c_int, 0i64);
    let r = unsafe { ol_play_thumb(c.as_ptr(), max_w, buf.as_mut_ptr(), cap, &mut w, &mut h, &mut dur) };
    if r != 0 || w <= 0 || h <= 0 {
        return None;
    }
    buf.truncate(w as usize * h as usize * 4);
    Some(Thumb { w: w as usize, h: h as usize, rgba: buf, duration_ms: dur })
}

struct Frame {
    pts_ms: i64,
    rgba: Vec<u8>,
}

struct Shared {
    frames: Mutex<VecDeque<Frame>>,
    audio: Mutex<VecDeque<i16>>,
    paused: AtomicBool,
    silent: AtomicBool,
    stop: AtomicBool,
    ended: AtomicBool,
    seek_v: AtomicI64,
    seek_a: AtomicI64,
    seeking: AtomicBool,
    /// seit dem letzten Springen abgespielte Ton-Werte (Stereo, 48 kHz)
    consumed: AtomicU64,
    base_ms: AtomicI64,
    volume: AtomicU32,
    error: Mutex<String>,
}

pub struct Player {
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
    athread: Option<std::thread::JoinHandle<()>>,
    _audio: Option<audio::Audio>,
    pub w: usize,
    pub h: usize,
    pub duration_ms: i64,
    pub has_audio: bool,
    /// Wiedergabeposition für Dateien ohne Ton (vom Bildschirm-Takt getrieben)
    soft_pos: f64,
    last_pos: i64,
    pub speed: f32,
    use_soft: bool,
    show_next: bool,
    frame_ms: i64,
    prev_pts: i64,
}

impl Player {
    pub fn open(path: &std::path::Path, volume: f32, device: &str) -> Result<Player, String> {
        let c = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        let mut err = [0 as c_char; 256];
        let p = unsafe { ol_play_open(c.as_ptr(), 1280, 1, err.as_mut_ptr(), err.len()) };
        if p.is_null() {
            return Err(crate::ffi::cstr(&err));
        }
        let (mut w, mut h, mut dur, mut ha) = (0 as c_int, 0 as c_int, 0i64, 0 as c_int);
        unsafe { ol_play_info(p, &mut w, &mut h, &mut dur, &mut ha) };
        let has_audio = ha != 0;
        // zweite Instanz nur für den Ton, damit Bild und Ton unabhängig von der Anordnung in der Datei laufen
        let pa = if has_audio {
            unsafe { ol_play_open(c.as_ptr(), 160, 2, err.as_mut_ptr(), err.len()) }
        } else {
            std::ptr::null_mut()
        };
        let has_audio = has_audio && !pa.is_null();
        let shared = Arc::new(Shared {
            frames: Mutex::new(VecDeque::new()),
            audio: Mutex::new(VecDeque::new()),
            paused: AtomicBool::new(false),
            silent: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            seek_v: AtomicI64::new(-1),
            seek_a: AtomicI64::new(-1),
            seeking: AtomicBool::new(false),
            consumed: AtomicU64::new(0),
            base_ms: AtomicI64::new(0),
            volume: AtomicU32::new(volume.clamp(0.0, 1.5).to_bits()),
            error: Mutex::new(String::new()),
        });
        let (wu, hu) = (w as usize, h as usize);

        // Bild-Thread
        let sh = shared.clone();
        let ptr = p as usize;
        let thread = std::thread::spawn(move || {
            let p = ptr as *mut OlPlayer;
            let cap = wu * hu * 4;
            let mut rgba = vec![0u8; cap];
            let mut aud = vec![0i16; 16];
            let mut skip_until: i64 = -1;
            loop {
                if sh.stop.load(Ordering::Relaxed) {
                    break;
                }
                let sk = sh.seek_v.swap(-1, Ordering::AcqRel);
                if sk >= 0 {
                    unsafe { ol_play_seek(p, sk) };
                    sh.frames.lock().unwrap().clear();
                    skip_until = sk;
                    sh.ended.store(false, Ordering::Release);
                }
                if sh.ended.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(15));
                    continue;
                }
                if sh.frames.lock().unwrap().len() >= 8 {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                let (mut vp, mut ap, mut an) = (0i64, 0i64, 0usize);
                let r = unsafe { ol_play_step(p, rgba.as_mut_ptr(), cap, &mut vp, aud.as_mut_ptr(), aud.len(), &mut an, &mut ap) };
                match r {
                    1 => {
                        if skip_until >= 0 && vp >= 0 && vp < skip_until - 20 {
                            continue;
                        }
                        skip_until = -1;
                        sh.frames.lock().unwrap().push_back(Frame { pts_ms: vp, rgba: rgba.clone() });
                    }
                    -1 => sh.ended.store(true, Ordering::Release),
                    -2 => {
                        *sh.error.lock().unwrap() = "Die Datei lässt sich nicht weiter abspielen.".to_string();
                        sh.ended.store(true, Ordering::Release);
                    }
                    _ => {}
                }
            }
            unsafe { ol_play_close(p) };
        });

        // Ton-Thread
        let athread = if has_audio {
            let sh = shared.clone();
            let pptr = pa as usize;
            Some(std::thread::spawn(move || {
                let p = pptr as *mut OlPlayer;
                let mut rgba = vec![0u8; 4];
                let mut aud = vec![0i16; 16384];
                let mut skip_until: i64 = -1;
                let mut a_ended = false;
                loop {
                    if sh.stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let sk = sh.seek_a.swap(-1, Ordering::AcqRel);
                    if sk >= 0 {
                        unsafe { ol_play_seek(p, sk) };
                        sh.audio.lock().unwrap().clear();
                        skip_until = sk;
                        a_ended = false;
                        sh.base_ms.store(sk, Ordering::Release);
                        sh.consumed.store(0, Ordering::Release);
                        sh.seeking.store(false, Ordering::Release);
                    }
                    if a_ended || sh.audio.lock().unwrap().len() > 48000 * 2 * 2 {
                        std::thread::sleep(Duration::from_millis(8));
                        continue;
                    }
                    let (mut vp, mut ap, mut an) = (0i64, 0i64, 0usize);
                    let r = unsafe { ol_play_step(p, rgba.as_mut_ptr(), 0, &mut vp, aud.as_mut_ptr(), aud.len(), &mut an, &mut ap) };
                    match r {
                        2 => {
                            if skip_until >= 0 && ap >= 0 && ap < skip_until - 20 {
                                continue;
                            }
                            skip_until = -1;
                            sh.audio.lock().unwrap().extend(aud[..an].iter().copied());
                        }
                        -1 | -2 => a_ended = true,
                        _ => {}
                    }
                }
                unsafe { ol_play_close(p) };
            }))
        } else {
            None
        };

        // Ton
        let audio_out = if has_audio {
            let sh = shared.clone();
            let pull: audio::Pull = Arc::new(move |buf: &mut [i16]| {
                if sh.paused.load(Ordering::Relaxed) || sh.seeking.load(Ordering::Relaxed) || sh.silent.load(Ordering::Relaxed) {
                    buf.iter_mut().for_each(|v| *v = 0);
                    return buf.len();
                }
                let vol = f32::from_bits(sh.volume.load(Ordering::Relaxed));
                let mut q = sh.audio.lock().unwrap();
                let mut got = 0usize;
                for v in buf.iter_mut() {
                    match q.pop_front() {
                        Some(s) => {
                            *v = ((s as f32) * vol).clamp(-32768.0, 32767.0) as i16;
                            got += 1;
                        }
                        None => *v = 0,
                    }
                }
                sh.consumed.fetch_add(got as u64, Ordering::Relaxed);
                buf.len()
            });
            audio::start_with(pull, device, 1.0).ok()
        } else {
            None
        };

        Ok(Player {
            shared,
            thread: Some(thread),
            athread,
            _audio: audio_out,
            w: wu,
            h: hu,
            duration_ms: dur,
            has_audio,
            soft_pos: 0.0,
            last_pos: 0,
            speed: 1.0,
            use_soft: false,
            show_next: false,
            frame_ms: 33,
            prev_pts: -1,
        })
    }

    pub fn debug_state(&self) -> String {
        format!(
            "frames {}  ton {}  verbraucht {}  ende {}  base {}",
            self.shared.frames.lock().unwrap().len(),
            self.shared.audio.lock().unwrap().len(),
            self.shared.consumed.load(Ordering::Relaxed),
            self.shared.ended.load(Ordering::Relaxed),
            self.shared.base_ms.load(Ordering::Relaxed)
        )
    }

    pub fn paused(&self) -> bool {
        self.shared.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&mut self, p: bool) {
        self.shared.paused.store(p, Ordering::Relaxed);
        // Nach Einzelbild-Schritten wieder normal (mit Ton) weiterspielen
        if !p && self.use_soft && self.speed == 1.0 && self.has_audio {
            self.use_soft = false;
            self.shared.silent.store(false, Ordering::Relaxed);
            let pos = self.last_pos;
            self.seek(pos);
        }
    }

    /// Wiedergabegeschwindigkeit (0.25, 0.5, 1.0, 2.0). Außer bei 1.0 ist der Ton stumm.
    pub fn set_speed(&mut self, v: f32) {
        if (v - self.speed).abs() < 0.001 {
            return;
        }
        self.speed = v;
        if v == 1.0 {
            if self.has_audio {
                self.use_soft = false;
                self.shared.silent.store(false, Ordering::Relaxed);
                let pos = self.last_pos;
                self.seek(pos);
            }
        } else {
            self.use_soft = true;
            self.soft_pos = self.last_pos as f64;
            self.shared.silent.store(true, Ordering::Relaxed);
        }
    }

    /// Ein Bild vor (+1) oder zurück (-1). Pausiert dabei.
    pub fn step_frame(&mut self, dir: i64) {
        self.shared.paused.store(true, Ordering::Relaxed);
        self.use_soft = true;
        self.shared.silent.store(true, Ordering::Relaxed);
        let target = (self.last_pos + dir * self.frame_ms).max(0);
        self.seek(target);
        self.show_next = true;
    }

    pub fn set_volume(&mut self, v: f32) {
        self.shared.volume.store(v.clamp(0.0, 1.5).to_bits(), Ordering::Relaxed);
    }

    pub fn ended(&self) -> bool {
        self.shared.ended.load(Ordering::Relaxed) && self.shared.frames.lock().unwrap().is_empty() && self.last_pos + 400 >= self.duration_ms.max(1)
    }

    pub fn error(&self) -> String {
        self.shared.error.lock().unwrap().clone()
    }

    /// Aktuelle Position in Millisekunden.
    pub fn position_ms(&self) -> i64 {
        self.last_pos
    }

    pub fn seek(&mut self, ms: i64) {
        let ms = ms.clamp(0, self.duration_ms.max(0));
        self.soft_pos = ms as f64;
        self.last_pos = ms;
        self.shared.frames.lock().unwrap().clear();
        if self.has_audio {
            self.shared.seeking.store(true, Ordering::Release);
            self.shared.seek_a.store(ms, Ordering::Release);
        }
        self.shared.seek_v.store(ms, Ordering::Release);
    }

    pub fn restart(&mut self) {
        self.seek(0);
        self.shared.paused.store(false, Ordering::Relaxed);
    }

    /// Ruft der Bildschirm einmal pro Bild auf. Gibt das aktuell fällige Videobild zurück, falls es ein neues gibt.
    pub fn poll(&mut self, dt: f32) -> Option<(usize, usize, Vec<u8>)> {
        let paused = self.paused();
        // Einzelbild-Schritt: das erste Bild nach dem Sprung zeigen, egal wie die Uhr steht
        if paused && self.show_next {
            let f = self.shared.frames.lock().unwrap().pop_front();
            if let Some(f) = f {
                self.show_next = false;
                self.last_pos = f.pts_ms.max(0);
                self.soft_pos = self.last_pos as f64;
                return Some((self.w, self.h, f.rgba));
            }
            return None;
        }
        let drained = self.shared.ended.load(Ordering::Relaxed) && (!self.has_audio || self.shared.audio.lock().unwrap().is_empty());
        let pos = if drained && !paused {
            i64::MAX / 4
        } else if self.has_audio && !self.use_soft {
            self.shared.base_ms.load(Ordering::Acquire) + (self.shared.consumed.load(Ordering::Relaxed) / 2 * 1000 / 48000) as i64
        } else {
            if !paused && !self.shared.frames.lock().unwrap().is_empty() {
                self.soft_pos += dt as f64 * 1000.0 * self.speed as f64;
            }
            self.soft_pos as i64
        };
        let mut newest: Option<Frame> = None;
        {
            let mut q = self.shared.frames.lock().unwrap();
            while let Some(f) = q.front() {
                if f.pts_ms <= pos || f.pts_ms < 0 {
                    newest = q.pop_front();
                } else {
                    break;
                }
            }
        }
        self.last_pos = if pos >= i64::MAX / 8 { self.duration_ms } else { pos.min(self.duration_ms.max(pos)) };
        if let Some(f) = &newest {
            if self.prev_pts >= 0 {
                let d = f.pts_ms - self.prev_pts;
                if (5..=100).contains(&d) {
                    self.frame_ms = d;
                }
            }
            self.prev_pts = f.pts_ms;
        }
        newest.map(|f| (self.w, self.h, f.rgba))
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        if let Some(t) = self.athread.take() {
            let _ = t.join();
        }
    }
}
