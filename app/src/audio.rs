//! Tonausgabe (WASAPI via cpal). Holt PCM aus dem chiaki-Kern.
use crate::ffi::Stream;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;

pub struct Audio {
    _stream: cpal::Stream,
}

pub fn start(stream: Arc<Stream>) -> Result<Audio, String> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("Kein Audiogerät gefunden")?;

    // Wunsch: 48 kHz, 2 Kanäle
    let mut chosen = None;
    if let Ok(configs) = device.supported_output_configs() {
        for c in configs {
            if c.channels() == 2 && c.min_sample_rate().0 <= 48000 && c.max_sample_rate().0 >= 48000 {
                chosen = Some(c.with_sample_rate(cpal::SampleRate(48000)));
                break;
            }
        }
    }
    let config = match chosen {
        Some(c) => c,
        None => device.default_output_config().map_err(|e| e.to_string())?,
    };
    let channels = config.channels() as usize;
    let sample_format = config.sample_format();
    let cfg: cpal::StreamConfig = config.into();

    let mut tmp = vec![0i16; 8192];
    let err_fn = |e| eprintln!("audio: {e}");

    let out = match sample_format {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &cfg,
            move |data: &mut [f32], _| fill_f32(&stream, data, channels, &mut tmp),
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => {
            let mut ftmp = vec![0f32; 8192];
            device.build_output_stream(
                &cfg,
                move |data: &mut [i16], _| {
                    if ftmp.len() < data.len() {
                        ftmp.resize(data.len(), 0.0);
                    }
                    fill_f32(&stream, &mut ftmp[..data.len()], channels, &mut tmp);
                    for (o, f) in data.iter_mut().zip(ftmp.iter()) {
                        *o = (*f * 32767.0) as i16;
                    }
                },
                err_fn,
                None,
            )
        }
        other => return Err(format!("Audioformat {other:?} wird nicht unterstützt")),
    }
    .map_err(|e| e.to_string())?;
    out.play().map_err(|e| e.to_string())?;
    Ok(Audio { _stream: out })
}

fn fill_f32(stream: &Stream, data: &mut [f32], channels: usize, tmp: &mut Vec<i16>) {
    // Quelle: Stereo (2 Kanäle). Ziel: `channels` Kanäle.
    let frames = data.len() / channels.max(1);
    let need = frames * 2;
    if tmp.len() < need {
        tmp.resize(need, 0);
    }
    let got = stream.take_audio(&mut tmp[..need]);
    let got_frames = got / 2;
    for f in 0..frames {
        let (l, r) = if f < got_frames {
            (tmp[f * 2] as f32 / 32768.0, tmp[f * 2 + 1] as f32 / 32768.0)
        } else {
            (0.0, 0.0)
        };
        for c in 0..channels {
            data[f * channels + c] = match c {
                0 => l,
                1 => r,
                _ => 0.0,
            };
        }
    }
}
