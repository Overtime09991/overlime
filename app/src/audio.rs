//! Tonausgabe (WASAPI via cpal). Holt PCM aus dem chiaki-Kern, mit Geräteauswahl,
//! Lautstärke und Anpassung an andere Abtastraten als 48 kHz.
use crate::ffi::Stream;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Gemeinsamer Zustand zwischen Oberfläche und Audio-Thread.
pub struct Shared {
    gain_bits: AtomicU32,
    eq_bits: [AtomicU32; 3],
    pub frames_played: AtomicU64,
    pub underruns: AtomicU64,
    pub info: Mutex<String>,
    pub error: Mutex<String>,
}

impl Shared {
    fn new(gain: f32) -> Self {
        Self {
            gain_bits: AtomicU32::new(gain.to_bits()),
            eq_bits: [AtomicU32::new(0f32.to_bits()), AtomicU32::new(0f32.to_bits()), AtomicU32::new(0f32.to_bits())],
            frames_played: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            info: Mutex::new(String::new()),
            error: Mutex::new(String::new()),
        }
    }
    pub fn set_gain(&self, g: f32) {
        self.gain_bits.store(g.max(0.0).to_bits(), Ordering::Relaxed);
    }
    /// Klang in dB (Bass, Mitten, Höhen), je -8 bis +8.
    pub fn set_eq(&self, bass: f32, mid: f32, treble: f32) {
        for (i, v) in [bass, mid, treble].iter().enumerate() {
            self.eq_bits[i].store(v.clamp(-8.0, 8.0).to_bits(), Ordering::Relaxed);
        }
    }
    fn eq(&self) -> [f32; 3] {
        [f32::from_bits(self.eq_bits[0].load(Ordering::Relaxed)), f32::from_bits(self.eq_bits[1].load(Ordering::Relaxed)), f32::from_bits(self.eq_bits[2].load(Ordering::Relaxed))]
    }
    fn gain(&self) -> f32 {
        f32::from_bits(self.gain_bits.load(Ordering::Relaxed))
    }
}

pub struct Audio {
    _stream: cpal::Stream,
    pub shared: Arc<Shared>,
}

/// Namen aller Ausgabegeräte (zum Auswählen).
pub fn list_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices().map(|it| it.filter_map(|d| d.name().ok()).collect()).unwrap_or_default()
}

pub fn default_device_name() -> String {
    cpal::default_host().default_output_device().and_then(|d| d.name().ok()).unwrap_or_default()
}

/// Quelle für Samples: füllt den Puffer mit interleaved Stereo-int16 bei 48 kHz, gibt die Anzahl zurück.
pub type Pull = Arc<dyn Fn(&mut [i16]) -> usize + Send + Sync>;

pub fn start(stream: Arc<Stream>, device_name: &str, gain: f32) -> Result<Audio, String> {
    let pull: Pull = Arc::new(move |buf: &mut [i16]| {
        let n = stream.take_audio(buf);
        if n >= 2 {
            crate::highlight::feed(&buf[..n]);
        }
        n
    });
    start_with(pull, device_name, gain)
}

/// Eine einfache lineare Anpassung von 48 kHz auf die Geräte-Abtastrate.
/// Ein Biquad-Filter (RBJ) für einen Kanal.
#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: [f32; 2],
    z2: [f32; 2],
}

impl Biquad {
    fn set(&mut self, kind: u8, fs: f32, f0: f32, q: f32, db: f32) {
        let a = 10f32.powf(db / 40.0);
        let w = 2.0 * std::f32::consts::PI * f0 / fs;
        let (sn, cs) = (w.sin(), w.cos());
        let alpha = sn / (2.0 * q);
        let (b0, b1, b2, a0, a1, a2);
        match kind {
            // Tiefen-Regler (Low-Shelf)
            0 => {
                let t = 2.0 * a.sqrt() * alpha;
                b0 = a * ((a + 1.0) - (a - 1.0) * cs + t);
                b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cs);
                b2 = a * ((a + 1.0) - (a - 1.0) * cs - t);
                a0 = (a + 1.0) + (a - 1.0) * cs + t;
                a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cs);
                a2 = (a + 1.0) + (a - 1.0) * cs - t;
            }
            // Mitten (Peaking)
            1 => {
                b0 = 1.0 + alpha * a;
                b1 = -2.0 * cs;
                b2 = 1.0 - alpha * a;
                a0 = 1.0 + alpha / a;
                a1 = -2.0 * cs;
                a2 = 1.0 - alpha / a;
            }
            // Höhen-Regler (High-Shelf)
            _ => {
                let t = 2.0 * a.sqrt() * alpha;
                b0 = a * ((a + 1.0) + (a - 1.0) * cs + t);
                b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cs);
                b2 = a * ((a + 1.0) + (a - 1.0) * cs - t);
                a0 = (a + 1.0) - (a - 1.0) * cs + t;
                a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cs);
                a2 = (a + 1.0) - (a - 1.0) * cs - t;
            }
        }
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }
    #[inline]
    fn run(&mut self, ch: usize, x: f32) -> f32 {
        let y = self.b0 * x + self.z1[ch];
        self.z1[ch] = self.b1 * x - self.a1 * y + self.z2[ch];
        self.z2[ch] = self.b2 * x - self.a2 * y;
        y
    }
}

/// Dreiband-Klangregler (Bass, Mitten, Höhen).
struct Eq {
    f: [Biquad; 3],
    cur: [f32; 3],
    pre: f32,
}

impl Eq {
    fn new() -> Self {
        Eq { f: [Biquad::default(); 3], cur: [0.0; 3], pre: 1.0 }
    }
    fn process(&mut self, out: &mut [f32], channels: usize, fs: f32, want: [f32; 3]) {
        if want != self.cur {
            self.cur = want;
            self.f[0].set(0, fs, 120.0, 0.8, want[0]);
            self.f[1].set(1, fs, 1000.0, 0.9, want[1]);
            self.f[2].set(2, fs, 6000.0, 0.8, want[2]);
            // Verstärkungen vorab absenken, damit nichts übersteuert
            let boost = want.iter().cloned().fold(0.0f32, f32::max);
            self.pre = 10f32.powf(-boost / 20.0);
        }
        if want == [0.0, 0.0, 0.0] {
            return;
        }
        let frames = out.len() / channels.max(1);
        for fr in 0..frames {
            for ch in 0..channels.min(2) {
                let mut x = out[fr * channels + ch] * self.pre;
                for b in self.f.iter_mut() {
                    x = b.run(ch, x);
                }
                out[fr * channels + ch] = x;
            }
        }
    }
}

struct Resampler {
    q: VecDeque<[f32; 2]>,
    h: [[f32; 2]; 4],
    t: f64,
}

/// Catmull-Rom zwischen p1 und p2.
fn cubic(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    p1 + 0.5 * t * (p2 - p0 + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)))
}

pub fn start_with(pull: Pull, device_name: &str, gain: f32) -> Result<Audio, String> {
    let host = cpal::default_host();
    let mut device = None;
    if !device_name.is_empty() {
        if let Ok(mut it) = host.output_devices() {
            device = it.find(|d| d.name().map(|n| n == device_name).unwrap_or(false));
        }
    }
    let used_default = device.is_none();
    let device = match device {
        Some(d) => d,
        None => host.default_output_device().ok_or("Kein Audiogerät gefunden")?,
    };
    let dev_name = device.name().unwrap_or_else(|_| "?".into());

    // Wunsch: 48 kHz, 2 Kanäle, bestes Format (F32 vor I16 vor I32 vor U16 vor U8).
    let mut best: Option<(u8, cpal::SupportedStreamConfig)> = None;
    if let Ok(configs) = device.supported_output_configs() {
        for c in configs {
            if c.channels() == 2 && c.min_sample_rate().0 <= 48000 && c.max_sample_rate().0 >= 48000 {
                let pref = match c.sample_format() {
                    cpal::SampleFormat::F32 => 5,
                    cpal::SampleFormat::I16 => 4,
                    cpal::SampleFormat::I32 => 3,
                    cpal::SampleFormat::U16 => 2,
                    cpal::SampleFormat::U8 => 1,
                    _ => 0,
                };
                if pref > 0 && best.as_ref().map(|(p, _)| pref > *p).unwrap_or(true) {
                    best = Some((pref, c.with_sample_rate(cpal::SampleRate(48000))));
                }
            }
        }
    }
    let config = match best {
        Some((_, c)) => c,
        None => device.default_output_config().map_err(|e| e.to_string())?,
    };
    let channels = (config.channels() as usize).max(1);
    let rate = config.sample_rate().0;
    let sample_format = config.sample_format();
    let cfg: cpal::StreamConfig = config.into();

    let shared = Arc::new(Shared::new(gain));
    *shared.info.lock().unwrap() = format!("{}{} • {} Hz • {} Kanäle", dev_name, if used_default { " (Standard)" } else { "" }, rate, channels);
    let ratio = 48000.0 / rate as f64;

    let sh = shared.clone();
    let sh_err = shared.clone();
    let err_fn = move |e: cpal::StreamError| {
        *sh_err.error.lock().unwrap() = e.to_string();
    };

    let mut tmp = vec![0i16; 16384];
    let mut rs = Resampler { q: VecDeque::new(), h: [[0.0; 2]; 4], t: 0.0 };
    let mut eq = Eq::new();

    let mut render = move |out: &mut [f32]| {
        let frames = out.len() / channels;
        let g = sh.gain();
        if (ratio - 1.0).abs() < 1e-6 {
            // direkt, ohne Anpassung
            let need = frames * 2;
            if tmp.len() < need {
                tmp.resize(need, 0);
            }
            let got = pull(&mut tmp[..need]);
            let gf = got / 2;
            if gf < frames {
                sh.underruns.fetch_add(1, Ordering::Relaxed);
            }
            sh.frames_played.fetch_add(gf as u64, Ordering::Relaxed);
            for f in 0..frames {
                let (l, r) = if f < gf { (tmp[f * 2] as f32 / 32768.0 * g, tmp[f * 2 + 1] as f32 / 32768.0 * g) } else { (0.0, 0.0) };
                for c in 0..channels {
                    out[f * channels + c] = match c { 0 => l, 1 => r, _ => 0.0 };
                }
            }
        } else {
            // Quelle auffüllen
            let need_src = ((frames as f64 * ratio).ceil() as usize + 8).max(16);
            if tmp.len() < need_src * 2 {
                tmp.resize(need_src * 2, 0);
            }
            let got = pull(&mut tmp[..need_src * 2]);
            for i in 0..got / 2 {
                rs.q.push_back([tmp[i * 2] as f32 / 32768.0, tmp[i * 2 + 1] as f32 / 32768.0]);
            }
            sh.frames_played.fetch_add((got / 2) as u64, Ordering::Relaxed);
            if rs.q.len() > 48000 {
                rs.q.clear();
            }
            let mut under = false;
            for f in 0..frames {
                let tt = rs.t as f32;
                let l = cubic(rs.h[0][0], rs.h[1][0], rs.h[2][0], rs.h[3][0], tt);
                let r = cubic(rs.h[0][1], rs.h[1][1], rs.h[2][1], rs.h[3][1], tt);
                for c in 0..channels {
                    out[f * channels + c] = match c { 0 => l * g, 1 => r * g, _ => 0.0 };
                }
                rs.t += ratio;
                while rs.t >= 1.0 {
                    rs.h[0] = rs.h[1];
                    rs.h[1] = rs.h[2];
                    rs.h[2] = rs.h[3];
                    match rs.q.pop_front() {
                        Some(n) => rs.h[3] = n,
                        None => under = true,
                    }
                    rs.t -= 1.0;
                }
            }
            if under {
                sh.underruns.fetch_add(1, Ordering::Relaxed);
            }
        }
        eq.process(out, channels, rate as f32, sh.eq());
    };

    let name = |f: cpal::SampleFormat| format!("{f:?}");
    let out = match sample_format {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, &cfg, render, |f| f, 0.0, err_fn),
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, &cfg, render, |f| (f.clamp(-1.0, 1.0) * 32767.0).round() as i16, 1.0 / 32768.0, err_fn),
        cpal::SampleFormat::I32 => build_stream::<i32>(&device, &cfg, render, |f| (f.clamp(-1.0, 1.0) as f64 * 2147483647.0) as i32, 0.0, err_fn),
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, &cfg, render, |f| ((f.clamp(-1.0, 1.0) + 1.0) * 0.5 * 65535.0).round() as u16, 1.0 / 32768.0, err_fn),
        cpal::SampleFormat::U8 => build_stream::<u8>(&device, &cfg, render, |f| ((f.clamp(-1.0, 1.0) + 1.0) * 0.5 * 255.0).round() as u8, 1.0 / 128.0, err_fn),
        other => return Err(format!("Audioformat {} wird nicht unterstützt", name(other))),
    }
    .map_err(|e| e.to_string())?;
    out.play().map_err(|e| e.to_string())?;
    *shared.info.lock().unwrap() += &format!(" • {}", name(sample_format));
    Ok(Audio { _stream: out, shared })
}

fn build_stream<T>(
    device: &cpal::Device,
    cfg: &cpal::StreamConfig,
    mut render: impl FnMut(&mut [f32]) + Send + 'static,
    conv: fn(f32) -> T,
    lsb: f32,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample + Send + 'static,
{
    let mut fbuf = vec![0f32; 16384];
    let mut seed: u32 = 0x9E37_79B9;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as f32 / u32::MAX as f32) - 0.5
    };
    device.build_output_stream(
        cfg,
        move |data: &mut [T], _| {
            if fbuf.len() < data.len() {
                fbuf.resize(data.len(), 0.0);
            }
            render(&mut fbuf[..data.len()]);
            for (o, f) in data.iter_mut().zip(fbuf.iter()) {
                // TPDF-Dither: verhindert Quantisierungs-Knistern bei 16/8 Bit
                let d = if lsb > 0.0 { (rnd() + rnd()) * lsb } else { 0.0 };
                *o = conv(*f + d);
            }
        },
        err_fn,
        None,
    )
}
