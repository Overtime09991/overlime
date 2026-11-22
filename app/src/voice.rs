//! Sprachchat: Verbindung zum OVERLIME-Sprachserver (WebSocket über TLS), Freunde, Räume und Sprache (Opus).
//! Die Technik hier ist unabhängig von der Oberfläche. Die Oberfläche steht in `friends.rs`.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::Message;

use crate::{audio, mic};

pub const DEFAULT_SERVER: &str = "wss://voice.overlime.info";
const AUTH_PREFIX: &[u8] = b"overlime-voice-v1";
/// 20 ms bei 48 kHz mono
const FRAME: usize = 960;

extern "C" {
    fn ol_opus_enc_new() -> *mut c_void;
    fn ol_opus_enc(e: *mut c_void, pcm: *const i16, frame: i32, out: *mut u8, cap: i32) -> i32;
    fn ol_opus_enc_free(e: *mut c_void);
    fn ol_opus_dec_new() -> *mut c_void;
    fn ol_opus_dec(d: *mut c_void, data: *const u8, len: i32, pcm: *mut i16, frame: i32) -> i32;
    fn ol_opus_dec_free(d: *mut c_void);
}

#[derive(Clone, Debug)]
pub struct Friend {
    pub id: String,
    pub name: String,
    pub in_room: bool,
}

#[derive(Clone, Debug)]
pub struct Member {
    pub id: String,
    pub name: String,
    pub idx: u8,
}

#[derive(Debug)]
pub enum Event {
    Connected(String),
    Disconnected(String),
    Presence(Vec<Friend>),
    Room { room: String, members: Vec<Member> },
    Invite { from: String, name: String, room: String },
    Error(String),
}

enum Cmd {
    Friends(Vec<String>),
    Join(Option<String>),
    Leave,
    Invite(String),
    Name(String),
}

struct Speaker {
    dec: usize,
    buf: VecDeque<i16>,
    primed: bool,
}

struct Shared {
    stop: AtomicBool,
    connected: AtomicBool,
    in_room: AtomicBool,
    /// Mikrofon sendet gerade (offenes Mikrofon oder Taste gedrückt)
    talking: AtomicBool,
    denoise: AtomicBool,
    vol_out: AtomicU32,
    out_q: Mutex<VecDeque<Vec<u8>>>,
    speakers: Mutex<HashMap<u8, Speaker>>,
    speaking: Mutex<HashMap<u8, Instant>>,
    idx_id: Mutex<HashMap<u8, String>>,
    blocked: Mutex<HashSet<String>>,
    decoded_energy: AtomicU64,
    sent_packets: AtomicU64,
}

pub struct Voice {
    shared: Arc<Shared>,
    cmd: Sender<Cmd>,
    events: Receiver<Event>,
    pub my_id: String,
    mic: Mutex<Option<mic::Mic>>,
    audio: Mutex<Option<audio::Audio>>,
    enc: Mutex<usize>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Aus dem öffentlichen Schlüssel einen kurzen Code machen: OL-XXXX-XXXX (muss zum Server passen).
pub fn make_id(pubkey: &[u8]) -> String {
    let h = Sha256::digest(pubkey);
    const ALPHA: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let mut bits: u64 = 0;
    for b in &h[..5] {
        bits = (bits << 8) | *b as u64;
    }
    let mut s = String::new();
    for i in 0..8 {
        let idx = ((bits >> (35 - 5 * i)) & 31) as usize % ALPHA.len();
        s.push(ALPHA[idx] as char);
        if i == 3 {
            s.push('-');
        }
    }
    format!("OL-{s}")
}

pub fn valid_id(s: &str) -> bool {
    s.len() == 12 && s.starts_with("OL-") && s.as_bytes()[7] == b'-' && s.chars().skip(3).all(|c| c == '-' || c.is_ascii_uppercase() || c.is_ascii_digit())
}

pub fn id_from_secret(secret: &[u8; 32]) -> String {
    make_id(&SigningKey::from_bytes(secret).verifying_key().to_bytes())
}

/// Neuer zufälliger geheimer Schlüssel.
pub fn new_secret() -> [u8; 32] {
    let mut b = [0u8; 32];
    // Zufall aus dem Betriebssystem
    let mut h = Sha256::new();
    for i in 0..4u32 {
        h.update(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0).to_le_bytes());
        h.update(i.to_le_bytes());
        h.update(std::process::id().to_le_bytes());
        let t = Instant::now();
        std::thread::sleep(Duration::from_micros(150));
        h.update(t.elapsed().as_nanos().to_le_bytes());
        h.update(format!("{:p}", &b).as_bytes());
    }
    let mut seed = h.finalize().to_vec();
    // zusätzlich der Windows-Zufallsgenerator (BCryptGenRandom)
    let mut os = [0u8; 32];
    if os_random(&mut os) {
        for (a, bb) in seed.iter_mut().zip(os.iter()) {
            *a ^= *bb;
        }
    }
    b.copy_from_slice(&Sha256::digest(&seed));
    b
}

#[cfg(windows)]
fn os_random(buf: &mut [u8]) -> bool {
    #[link(name = "bcrypt")]
    extern "system" {
        fn BCryptGenRandom(h: *mut c_void, buf: *mut u8, len: u32, flags: u32) -> i32;
    }
    unsafe { BCryptGenRandom(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32, 2) == 0 }
}

#[cfg(not(windows))]
fn os_random(_buf: &mut [u8]) -> bool {
    false
}

impl Voice {
    pub fn start(secret: [u8; 32], name: &str, server: &str) -> Voice {
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            connected: AtomicBool::new(false),
            in_room: AtomicBool::new(false),
            talking: AtomicBool::new(false),
            denoise: AtomicBool::new(true),
            vol_out: AtomicU32::new(1.0f32.to_bits()),
            out_q: Mutex::new(VecDeque::new()),
            speakers: Mutex::new(HashMap::new()),
            speaking: Mutex::new(HashMap::new()),
            idx_id: Mutex::new(HashMap::new()),
            blocked: Mutex::new(HashSet::new()),
            decoded_energy: AtomicU64::new(0),
            sent_packets: AtomicU64::new(0),
        });
        let (ctx, crx) = channel::<Cmd>();
        let (etx, erx) = channel::<Event>();
        let my_id = id_from_secret(&secret);
        let sh = shared.clone();
        let name = name.to_string();
        let server = server.to_string();
        let thread = std::thread::spawn(move || net_loop(secret, name, server, sh, crx, etx));
        Voice { shared, cmd: ctx, events: erx, my_id, mic: Mutex::new(None), audio: Mutex::new(None), enc: Mutex::new(0), thread: Some(thread) }
    }

    pub fn connected(&self) -> bool {
        self.shared.connected.load(Ordering::Relaxed)
    }

    pub fn in_room(&self) -> bool {
        self.shared.in_room.load(Ordering::Relaxed)
    }

    pub fn poll(&self) -> Vec<Event> {
        let mut v = Vec::new();
        while let Ok(e) = self.events.try_recv() {
            v.push(e);
        }
        v
    }

    pub fn set_friends(&self, ids: Vec<String>) {
        let _ = self.cmd.send(Cmd::Friends(ids));
    }

    pub fn set_name(&self, n: &str) {
        let _ = self.cmd.send(Cmd::Name(n.to_string()));
    }

    pub fn join(&self, room: Option<String>) {
        let _ = self.cmd.send(Cmd::Join(room));
    }

    pub fn leave(&self) {
        let _ = self.cmd.send(Cmd::Leave);
        self.stop_audio();
    }

    pub fn invite(&self, id: &str) {
        let _ = self.cmd.send(Cmd::Invite(id.to_string()));
    }

    pub fn set_blocked(&self, ids: Vec<String>) {
        *self.shared.blocked.lock().unwrap() = ids.into_iter().collect();
    }

    pub fn set_denoise(&self, on: bool) {
        self.shared.denoise.store(on, Ordering::Relaxed);
    }

    pub fn set_talking(&self, on: bool) {
        self.shared.talking.store(on, Ordering::Relaxed);
    }

    pub fn talking(&self) -> bool {
        self.shared.talking.load(Ordering::Relaxed)
    }

    pub fn set_volume(&self, v: f32) {
        self.shared.vol_out.store(v.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }

    /// Pegel des Mikrofons (0..1), falls es läuft
    pub fn mic_level(&self) -> f32 {
        self.mic.lock().unwrap().as_ref().map(|m| m.level()).unwrap_or(0.0)
    }

    pub fn mic_info_error(&self) -> Option<String> {
        self.mic.lock().unwrap().as_ref().map(|m| m.error.lock().unwrap().clone()).filter(|e| !e.is_empty())
    }

    /// Wer spricht gerade (Raum-Nummer → ja/nein, innerhalb der letzten 400 ms)
    pub fn speaking_idx(&self) -> HashSet<u8> {
        let now = Instant::now();
        self.shared.speaking.lock().unwrap().iter().filter(|(_, t)| now.duration_since(**t) < Duration::from_millis(400)).map(|(i, _)| *i).collect()
    }

    /// Startet Mikrofon und Wiedergabe (beim Betreten eines Raums).
    pub fn start_audio(&self, mic_device: &str, mic_gain: f32, mic_gate: f32, denoise: bool, out_device: &str, out_gain: f32) -> Result<(), String> {
        self.shared.denoise.store(denoise, Ordering::Relaxed);
        self.stop_audio();
        // Wiedergabe
        let sh = self.shared.clone();
        let pull: audio::Pull = Arc::new(move |buf: &mut [i16]| mix_playback(&sh, buf));
        let a = audio::start_with(pull, out_device, out_gain)?;
        *self.audio.lock().unwrap() = Some(a);
        // Mikrofon -> Opus -> Server
        let enc = unsafe { ol_opus_enc_new() } as usize;
        if enc == 0 {
            return Err("Opus-Kodierer fehlt".into());
        }
        *self.enc.lock().unwrap() = enc;
        let sh = self.shared.clone();
        let acc: Mutex<Vec<i16>> = Mutex::new(Vec::with_capacity(FRAME * 2));
        let dn: Mutex<(Box<nnnoiseless::DenoiseState<'static>>, u32)> = Mutex::new((nnnoiseless::DenoiseState::new(), 0));
        let sink: mic::Sink = Arc::new(move |pcm: &[i16]| {
            if !sh.talking.load(Ordering::Relaxed) || !sh.in_room.load(Ordering::Relaxed) {
                acc.lock().unwrap().clear();
                return;
            }
            let mut a = acc.lock().unwrap();
            if sh.denoise.load(Ordering::Relaxed) && pcm.len() >= 960 {
                // Rauschfilter (RNNoise): Rauschen heraus, Sprache bleibt. Die Sprecher-Erkennung öffnet und schließt die Sperre.
                let inp: Vec<f32> = pcm.chunks(2).map(|fr| fr[0] as f32).collect();
                let mut out = vec![0.0f32; inp.len()];
                let (state, hold) = &mut *dn.lock().unwrap();
                let vad = state.process_frame(&mut out, &inp);
                if vad > 0.4 {
                    *hold = 30;
                } else if *hold > 0 {
                    *hold -= 1;
                }
                let open = *hold > 0;
                for v in out.iter() {
                    a.push(if open { v.clamp(-32768.0, 32767.0) as i16 } else { 0 });
                }
            } else {
                for fr in pcm.chunks(2) {
                    a.push(fr[0]);
                }
            }
            while a.len() >= FRAME {
                let frame: Vec<i16> = a.drain(..FRAME).collect();
                let mut out = vec![0u8; 400];
                out[0] = 1;
                let n = unsafe { ol_opus_enc(enc as *mut c_void, frame.as_ptr(), FRAME as i32, out.as_mut_ptr().add(1), 399) };
                if n > 0 {
                    out.truncate(1 + n as usize);
                    let mut q = sh.out_q.lock().unwrap();
                    if q.len() < 50 {
                        q.push_back(out);
                        sh.sent_packets.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
        match mic::start(mic_device, mic_gain, if denoise { 0.0 } else { mic_gate }, sink) {
            Ok(m) => {
                *self.mic.lock().unwrap() = Some(m);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn update_mic(&self, gain: f32, gate: f32, denoise: bool) {
        self.shared.denoise.store(denoise, Ordering::Relaxed);
        if let Some(m) = self.mic.lock().unwrap().as_ref() {
            m.set_gain(gain);
            m.set_gate(if denoise { 0.0 } else { gate });
        }
    }

    pub fn stop_audio(&self) {
        *self.mic.lock().unwrap() = None;
        *self.audio.lock().unwrap() = None;
        let mut e = self.enc.lock().unwrap();
        if *e != 0 {
            // Der Mikrofon-Strom ist oben schon beendet, der Kodierer kann weg
            unsafe { ol_opus_enc_free(*e as *mut c_void) };
            *e = 0;
        }
        self.shared.speaking.lock().unwrap().clear();
        let mut sp = self.shared.speakers.lock().unwrap();
        for (_, s) in sp.drain() {
            unsafe { ol_opus_dec_free(s.dec as *mut c_void) };
        }
    }

    /// Nur für den Selbsttest: ein fertiges 20-ms-Paket einspeisen (statt Mikrofon).
    pub fn test_send_tone(&self, phase: &mut f32) {
        let enc = unsafe { ol_opus_enc_new() };
        let mut pcm = [0i16; FRAME];
        for s in pcm.iter_mut() {
            *s = ((*phase).sin() * 8000.0) as i16;
            *phase += 2.0 * std::f32::consts::PI * 440.0 / 48000.0;
        }
        let mut out = vec![0u8; 400];
        out[0] = 1;
        let n = unsafe { ol_opus_enc(enc, pcm.as_ptr(), FRAME as i32, out.as_mut_ptr().add(1), 399) };
        unsafe { ol_opus_enc_free(enc) };
        if n > 0 {
            out.truncate(1 + n as usize);
            self.shared.out_q.lock().unwrap().push_back(out);
        }
    }

    pub fn test_decoded_energy(&self) -> u64 {
        self.shared.decoded_energy.load(Ordering::Relaxed)
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.stop_audio();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Mischt alle Sprecher zu Stereo-48-kHz für die Wiedergabe.
fn mix_playback(sh: &Arc<Shared>, buf: &mut [i16]) -> usize {
    let frames = buf.len() / 2;
    let vol = f32::from_bits(sh.vol_out.load(Ordering::Relaxed));
    let mut mix = vec![0i32; frames];
    let mut sp = sh.speakers.lock().unwrap();
    for s in sp.values_mut() {
        // erst ab 60 ms Vorrat abspielen (Ausgleich von Schwankungen), später bis leer
        if !s.primed {
            if s.buf.len() >= 2880 {
                s.primed = true;
            } else {
                continue;
            }
        }
        for m in mix.iter_mut() {
            match s.buf.pop_front() {
                Some(v) => *m += v as i32,
                None => {
                    s.primed = false;
                    break;
                }
            }
        }
        // nicht zu viel aufstauen (höchstens 400 ms)
        while s.buf.len() > 19200 {
            s.buf.pop_front();
        }
    }
    for (i, m) in mix.iter().enumerate() {
        let v = ((*m as f32) * vol).clamp(-32768.0, 32767.0) as i16;
        buf[i * 2] = v;
        buf[i * 2 + 1] = v;
    }
    buf.len()
}

type Ws = tungstenite::WebSocket<MaybeTlsStream<std::net::TcpStream>>;

fn set_timeout(ws: &mut Ws, ms: u64) {
    let d = Some(Duration::from_millis(ms));
    match ws.get_mut() {
        MaybeTlsStream::Plain(s) => {
            let _ = s.set_read_timeout(d);
            let _ = s.set_nodelay(true);
        }
        MaybeTlsStream::Rustls(s) => {
            let _ = s.sock.set_read_timeout(d);
            let _ = s.sock.set_nodelay(true);
        }
        _ => {}
    }
}

fn is_timeout(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io) if matches!(io.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
}

fn net_loop(secret: [u8; 32], mut name: String, server: String, sh: Arc<Shared>, crx: Receiver<Cmd>, etx: Sender<Event>) {
    let key = SigningKey::from_bytes(&secret);
    let mut friends: Vec<String> = Vec::new();
    let mut backoff = 1u64;
    while !sh.stop.load(Ordering::Relaxed) {
        // Verbinden
        let conn = tungstenite::connect(server.as_str());
        let mut ws: Ws = match conn {
            Ok((w, _)) => w,
            Err(e) => {
                let _ = etx.send(Event::Disconnected(format!("Keine Verbindung zum Sprachserver ({e})")));
                for _ in 0..backoff * 10 {
                    if sh.stop.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                backoff = (backoff * 2).min(30);
                continue;
            }
        };
        set_timeout(&mut ws, 20);
        // Anmeldung
        let mut ok = false;
        let t0 = Instant::now();
        let mut nonce: Option<Vec<u8>> = None;
        while t0.elapsed() < Duration::from_secs(10) && !sh.stop.load(Ordering::Relaxed) {
            match ws.read() {
                Ok(Message::Text(t)) => {
                    let v: Value = serde_json::from_str(&t).unwrap_or(Value::Null);
                    match v["t"].as_str() {
                        Some("hello") => {
                            nonce = v["nonce"].as_str().and_then(|n| B64.decode(n).ok());
                            if let Some(n) = &nonce {
                                let mut msg = AUTH_PREFIX.to_vec();
                                msg.extend_from_slice(n);
                                let sig = key.sign(&msg);
                                let _ = ws.send(Message::Text(json!({ "t": "auth", "pub": B64.encode(key.verifying_key().to_bytes()), "sig": B64.encode(sig.to_bytes()), "name": name }).to_string()));
                            }
                        }
                        Some("ok") => {
                            ok = true;
                            break;
                        }
                        Some("error") => {
                            let _ = etx.send(Event::Error(v["msg"].as_str().unwrap_or("Fehler").to_string()));
                            break;
                        }
                        _ => {}
                    }
                }
                Ok(_) => {}
                Err(e) if is_timeout(&e) => {}
                Err(_) => break,
            }
        }
        if !ok {
            let _ = etx.send(Event::Disconnected("Anmeldung beim Sprachserver fehlgeschlagen".into()));
            std::thread::sleep(Duration::from_secs(3));
            continue;
        }
        backoff = 1;
        sh.connected.store(true, Ordering::Relaxed);
        let _ = etx.send(Event::Connected(id_from_secret(&secret)));
        let _ = ws.send(Message::Text(json!({ "t": "friends", "ids": friends }).to_string()));
        let mut last_ping = Instant::now();
        let mut reason = String::from("Verbindung beendet");
        'conn: loop {
            if sh.stop.load(Ordering::Relaxed) {
                let _ = ws.close(None);
                break;
            }
            // Befehle
            while let Ok(c) = crx.try_recv() {
                let msg = match c {
                    Cmd::Friends(ids) => {
                        friends = ids.clone();
                        json!({ "t": "friends", "ids": ids })
                    }
                    Cmd::Join(r) => {
                        sh.in_room.store(true, Ordering::Relaxed);
                        match r {
                            Some(r) => json!({ "t": "join", "room": r }),
                            None => json!({ "t": "join" }),
                        }
                    }
                    Cmd::Leave => {
                        sh.in_room.store(false, Ordering::Relaxed);
                        json!({ "t": "leave" })
                    }
                    Cmd::Invite(to) => json!({ "t": "invite", "to": to }),
                    Cmd::Name(n) => {
                        name = n.clone();
                        json!({ "t": "name", "name": n })
                    }
                };
                if ws.send(Message::Text(msg.to_string())).is_err() {
                    reason = "Verbindung verloren".into();
                    break 'conn;
                }
            }
            // Ausgehende Sprache
            loop {
                let p = sh.out_q.lock().unwrap().pop_front();
                match p {
                    Some(p) => {
                        if ws.send(Message::Binary(p)).is_err() {
                            reason = "Verbindung verloren".into();
                            break 'conn;
                        }
                    }
                    None => break,
                }
            }
            if last_ping.elapsed() > Duration::from_secs(15) {
                last_ping = Instant::now();
                if ws.send(Message::Text(json!({ "t": "ping" }).to_string())).is_err() {
                    reason = "Verbindung verloren".into();
                    break 'conn;
                }
            }
            // Eingehend
            match ws.read() {
                Ok(Message::Text(t)) => {
                    let v: Value = serde_json::from_str(&t).unwrap_or(Value::Null);
                    match v["t"].as_str() {
                        Some("presence") => {
                            let list = v["online"]
                                .as_array()
                                .map(|a| a.iter().filter_map(|f| Some(Friend { id: f["id"].as_str()?.to_string(), name: f["name"].as_str().unwrap_or("").to_string(), in_room: f["room"].as_bool().unwrap_or(false) })).collect())
                                .unwrap_or_default();
                            let _ = etx.send(Event::Presence(list));
                        }
                        Some("room") => {
                            let room = v["room"].as_str().unwrap_or("").to_string();
                            let members: Vec<Member> = v["members"]
                                .as_array()
                                .map(|a| a.iter().filter_map(|m| Some(Member { id: m["id"].as_str()?.to_string(), name: m["name"].as_str().unwrap_or("").to_string(), idx: m["idx"].as_u64()? as u8 })).collect())
                                .unwrap_or_default();
                            *sh.idx_id.lock().unwrap() = members.iter().map(|m| (m.idx, m.id.clone())).collect();
                            sh.in_room.store(!room.is_empty(), Ordering::Relaxed);
                            // Sprecher, die gegangen sind, aufräumen
                            let keep: HashSet<u8> = members.iter().map(|m| m.idx).collect();
                            let mut sp = sh.speakers.lock().unwrap();
                            let gone: Vec<u8> = sp.keys().filter(|k| !keep.contains(k)).cloned().collect();
                            for g in gone {
                                if let Some(s) = sp.remove(&g) {
                                    unsafe { ol_opus_dec_free(s.dec as *mut c_void) };
                                }
                            }
                            drop(sp);
                            let _ = etx.send(Event::Room { room, members });
                        }
                        Some("invite") => {
                            let _ = etx.send(Event::Invite { from: v["from"].as_str().unwrap_or("").to_string(), name: v["name"].as_str().unwrap_or("").to_string(), room: v["room"].as_str().unwrap_or("").to_string() });
                        }
                        Some("error") => {
                            let msg = v["msg"].as_str().unwrap_or("Fehler").to_string();
                            if msg.contains("anderer Stelle") {
                                reason = msg.clone();
                                let _ = etx.send(Event::Error(msg));
                                break 'conn;
                            }
                            let _ = etx.send(Event::Error(msg));
                        }
                        _ => {}
                    }
                }
                Ok(Message::Binary(d)) => {
                    if d.len() < 2 {
                        continue;
                    }
                    let idx = d[0];
                    // blockierte Leute nicht abspielen
                    let blocked = {
                        let id = sh.idx_id.lock().unwrap().get(&idx).cloned();
                        id.map(|i| sh.blocked.lock().unwrap().contains(&i)).unwrap_or(false)
                    };
                    if blocked {
                        continue;
                    }
                    let mut sp = sh.speakers.lock().unwrap();
                    let s = sp.entry(idx).or_insert_with(|| Speaker { dec: unsafe { ol_opus_dec_new() } as usize, buf: VecDeque::new(), primed: false });
                    if s.dec == 0 {
                        continue;
                    }
                    let mut pcm = vec![0i16; 5760];
                    let n = unsafe { ol_opus_dec(s.dec as *mut c_void, d[1..].as_ptr(), (d.len() - 1) as i32, pcm.as_mut_ptr(), 5760) };
                    if n > 0 {
                        let n = n as usize;
                        let e: u64 = pcm[..n].iter().map(|v| v.unsigned_abs() as u64).sum();
                        sh.decoded_energy.fetch_add(e, Ordering::Relaxed);
                        s.buf.extend(pcm[..n].iter().copied());
                        drop(sp);
                        sh.speaking.lock().unwrap().insert(idx, Instant::now());
                    }
                }
                Ok(Message::Close(_)) => {
                    reason = "Der Server hat die Verbindung beendet".into();
                    break 'conn;
                }
                Ok(_) => {}
                Err(e) if is_timeout(&e) => {}
                Err(_) => {
                    reason = "Verbindung verloren".into();
                    break 'conn;
                }
            }
        }
        sh.connected.store(false, Ordering::Relaxed);
        sh.in_room.store(false, Ordering::Relaxed);
        let _ = etx.send(Event::Disconnected(reason));
        if sh.stop.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}
