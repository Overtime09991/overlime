//! Mikrofon für die Sprache zur Konsole (Party-Chat): nimmt vom gewählten Gerät auf, wandelt auf 48 kHz Stereo,
//! wendet Lautstärke und Rauschsperre an und liefert 10-ms-Blöcke (480 Abtastwerte) an einen Empfänger.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

pub type Sink = Arc<dyn Fn(&[i16]) + Send + Sync>;

pub struct Mic {
    _stream: cpal::Stream,
    /// Pegel 0..1 (geglättet), als f32-Bits
    pub level: Arc<AtomicU32>,
    gain: Arc<AtomicU32>,
    gate: Arc<AtomicU32>,
    pub info: String,
    pub error: Arc<Mutex<String>>,
}

pub fn list_inputs() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices().map(|it| it.filter_map(|d| d.name().ok()).collect()).unwrap_or_default()
}

pub fn default_input_name() -> String {
    cpal::default_host().default_input_device().and_then(|d| d.name().ok()).unwrap_or_default()
}

impl Mic {
    pub fn set_gain(&self, g: f32) {
        self.gain.store(g.to_bits(), Ordering::Relaxed);
    }

    pub fn set_gate(&self, g: f32) {
        self.gate.store(g.to_bits(), Ordering::Relaxed);
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
}

/// Startet die Aufnahme. `on_frame` bekommt 960 Werte (480 Stereo-Abtastwerte, interleaved).
pub fn start(device_name: &str, gain: f32, gate: f32, on_frame: Sink) -> Result<Mic, String> {
    let host = cpal::default_host();
    let mut device = None;
    if !device_name.is_empty() {
        if let Ok(mut it) = host.input_devices() {
            device = it.find(|d| d.name().map(|n| n == device_name).unwrap_or(false));
        }
    }
    let used_default = device.is_none();
    let device = match device {
        Some(d) => d,
        None => host.default_input_device().ok_or("Kein Mikrofon gefunden")?,
    };
    let dev_name = device.name().unwrap_or_else(|_| "?".into());
    let config = device.default_input_config().map_err(|e| e.to_string())?;
    let channels = (config.channels() as usize).max(1);
    let rate = config.sample_rate().0 as f64;
    let fmt = config.sample_format();
    let cfg: cpal::StreamConfig = config.into();

    let level = Arc::new(AtomicU32::new(0f32.to_bits()));
    let gain_a = Arc::new(AtomicU32::new(gain.to_bits()));
    let gate_a = Arc::new(AtomicU32::new(gate.to_bits()));
    let error = Arc::new(Mutex::new(String::new()));
    let info = format!("{}{} • {} Hz • {} Kanäle", dev_name, if used_default { " (Standard)" } else { "" }, rate as u32, channels);

    // Verarbeitung: mono mischen, linear auf 48 kHz, Blöcke zu 480
    let lv = level.clone();
    let ga = gain_a.clone();
    let gt = gate_a.clone();
    let mut q: VecDeque<f32> = VecDeque::new();
    let mut pos = 0.0f64; // Position im Eingabestrom
    let mut inbuf: Vec<f32> = Vec::new();
    let step = rate / 48000.0;
    let mut hold = 0u32; // Rauschsperre bleibt noch kurz offen
    let mut smooth = 0.0f32;
    let mut mono_in: Vec<f32> = Vec::new();
    let mut process = move |data: &[f32]| {
        mono_in.clear();
        for fr in data.chunks(channels) {
            mono_in.push(fr.iter().sum::<f32>() / channels as f32);
        }
        // Resampling (linear) auf 48 kHz
        inbuf.extend_from_slice(&mono_in);
        while pos + 1.0 < inbuf.len() as f64 {
            let i = pos as usize;
            let frac = (pos - i as f64) as f32;
            q.push_back(inbuf[i] * (1.0 - frac) + inbuf[i + 1] * frac);
            pos += step;
        }
        let used = (pos as usize).min(inbuf.len());
        inbuf.drain(..used);
        pos -= used as f64;
        while q.len() >= 480 {
            let g = f32::from_bits(ga.load(Ordering::Relaxed));
            let gate = f32::from_bits(gt.load(Ordering::Relaxed));
            let frame: Vec<f32> = q.drain(..480).map(|v| v * g).collect();
            let rms = (frame.iter().map(|v| v * v).sum::<f32>() / 480.0).sqrt();
            let peak = frame.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            smooth = smooth * 0.8 + peak.min(1.0) * 0.2;
            lv.store(smooth.to_bits(), Ordering::Relaxed);
            // Rauschsperre mit kurzem Nachhalten (200 ms)
            if rms >= gate {
                hold = 20;
            }
            let open = hold > 0;
            hold = hold.saturating_sub(1);
            let mut out = vec![0i16; 960];
            if open {
                for (i, v) in frame.iter().enumerate() {
                    let s = (v.clamp(-1.0, 1.0) * 32767.0) as i16;
                    out[i * 2] = s;
                    out[i * 2 + 1] = s;
                }
            }
            on_frame(&out);
        }
    };

    let err_state = error.clone();
    let err_fn = move |e: cpal::StreamError| {
        *err_state.lock().unwrap() = e.to_string();
    };
    let stream = match fmt {
        cpal::SampleFormat::F32 => device.build_input_stream(&cfg, move |d: &[f32], _| process(d), err_fn, None),
        cpal::SampleFormat::I16 => {
            let mut tmp: Vec<f32> = Vec::new();
            device.build_input_stream(
                &cfg,
                move |d: &[i16], _| {
                    tmp.clear();
                    tmp.extend(d.iter().map(|v| *v as f32 / 32768.0));
                    process(&tmp)
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let mut tmp: Vec<f32> = Vec::new();
            device.build_input_stream(
                &cfg,
                move |d: &[u16], _| {
                    tmp.clear();
                    tmp.extend(d.iter().map(|v| (*v as f32 - 32768.0) / 32768.0));
                    process(&tmp)
                },
                err_fn,
                None,
            )
        }
        other => return Err(format!("Mikrofon-Format nicht unterstützt: {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(Mic { _stream: stream, level, gain: gain_a, gate: gate_a, info, error })
}
