//! Rohe Anbindung an overlime_core (C) + sichere Hüllen.
use std::ffi::{c_char, c_int, CStr, CString};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct OlRegistered {
    pub target: c_int,
    pub ps5: c_int,
    pub nickname: [c_char; 0x20],
    pub rp_regist_key: [u8; 16],
    pub rp_key: [u8; 16],
    pub rp_key_type: u32,
}

#[repr(C)]
pub struct OlStreamParams {
    pub host: [c_char; 128],
    pub ps5: c_int,
    pub rp_regist_key: [u8; 16],
    pub rp_key: [u8; 16],
    pub width: c_int,
    pub height: c_int,
    pub fps: c_int,
    pub bitrate_kbps: c_int,
    pub codec: c_int,
    pub hw_decode: c_int,
}

#[repr(C)]
pub struct OlStatus {
    pub state: c_int,
    pub quit_error: c_int,
    pub message: [c_char; 256],
    pub frames_decoded: u64,
    pub width: c_int,
    pub height: c_int,
    pub audio_rate: u32,
    pub audio_channels: u32,
    pub frames_lost: i32,
    pub audio_total: u64,
}

#[repr(C)]
pub struct OlStream {
    _private: [u8; 0],
}

extern "C" {
    pub fn chiaki_lib_init() -> c_int;
    pub fn ol_init();
    pub fn ol_log_take(buf: *mut c_char, size: usize) -> usize;
    pub fn ol_register(
        host: *const c_char,
        system_version: *const c_char,
        ps5: c_int,
        account_id: *const u8,
        pin: u32,
        out: *mut OlRegistered,
        err: *mut c_char,
        err_size: usize,
        timeout_ms: c_int,
    ) -> c_int;
    pub fn ol_stream_start(p: *const OlStreamParams, err: *mut c_char, err_size: usize) -> *mut OlStream;
    pub fn ol_stream_status(s: *mut OlStream, out: *mut OlStatus);
    pub fn ol_stream_take_frame(s: *mut OlStream, buf: *mut u8, size: usize, w: *mut c_int, h: *mut c_int) -> c_int;
    pub fn ol_stream_take_audio(s: *mut OlStream, buf: *mut i16, max: usize) -> usize;
    pub fn ol_stream_set_input(s: *mut OlStream, buttons: u32, l2: u8, r2: u8, lx: i16, ly: i16, rx: i16, ry: i16);
    pub fn ol_stream_take_rumble(s: *mut OlStream, left: *mut u8, right: *mut u8) -> c_int;
    pub fn ol_stream_stop(s: *mut OlStream);
}

pub fn cstr(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).to_string()
}

/// Holt gesammelte chiaki-Logzeilen (für Fehlersuche).
pub fn take_log() -> String {
    let mut buf = vec![0 as c_char; 8192];
    let n = unsafe { ol_log_take(buf.as_mut_ptr(), buf.len()) };
    if n == 0 {
        return String::new();
    }
    cstr(&buf)
}

pub fn init() {
    unsafe {
        chiaki_lib_init();
        ol_init()
    }
}

/// Eine registrierte (gekoppelte) Konsole.
#[derive(Clone, Debug)]
pub struct Paired {
    pub console_id: String,
    pub ps5: bool,
    pub nickname: String,
    pub rp_regist_key: [u8; 16],
    pub rp_key: [u8; 16],
}

/// Blockierend! In einem Thread aufrufen.
pub fn register(
    host: &str,
    system_version: &str,
    ps5: bool,
    account_id: [u8; 8],
    pin: u32,
    console_id: &str,
) -> Result<Paired, String> {
    let host_c = CString::new(host).map_err(|e| e.to_string())?;
    let ver_c = CString::new(system_version).map_err(|e| e.to_string())?;
    let mut out: OlRegistered = unsafe { std::mem::zeroed() };
    let mut err = vec![0 as c_char; 256];
    let rc = unsafe {
        ol_register(host_c.as_ptr(), ver_c.as_ptr(), ps5 as c_int, account_id.as_ptr(), pin, &mut out, err.as_mut_ptr(), err.len(), 15000)
    };
    if rc != 0 {
        return Err(cstr(&err));
    }
    Ok(Paired {
        console_id: console_id.to_string(),
        ps5,
        nickname: cstr(&out.nickname),
        rp_regist_key: out.rp_regist_key,
        rp_key: out.rp_key,
    })
}

impl Paired {
    /// Aufwecken-Credential: Hex-Zahl aus dem Regist-Key.
    pub fn wake_credential(&self) -> Option<u64> {
        let end = self.rp_regist_key.iter().position(|&b| b == 0).unwrap_or(16);
        let s = std::str::from_utf8(&self.rp_regist_key[..end]).ok()?;
        if s.is_empty() || s.len() > 16 {
            return None;
        }
        u64::from_str_radix(s, 16).ok()
    }
}

pub struct Stream {
    ptr: *mut OlStream,
}
unsafe impl Send for Stream {}
unsafe impl Sync for Stream {}

pub struct StreamParams {
    pub host: String,
    pub paired: Paired,
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub bitrate_kbps: i32,
    pub hevc: bool,
    pub hw_decode: bool,
}

impl Stream {
    pub fn start(p: &StreamParams) -> Result<Stream, String> {
        let mut params: OlStreamParams = unsafe { std::mem::zeroed() };
        for (i, b) in p.host.bytes().take(127).enumerate() {
            params.host[i] = b as c_char;
        }
        params.ps5 = p.paired.ps5 as c_int;
        params.rp_regist_key = p.paired.rp_regist_key;
        params.rp_key = p.paired.rp_key;
        params.width = p.width;
        params.height = p.height;
        params.fps = p.fps;
        params.bitrate_kbps = p.bitrate_kbps;
        params.codec = p.hevc as c_int;
        params.hw_decode = p.hw_decode as c_int;
        let mut err = vec![0 as c_char; 256];
        let ptr = unsafe { ol_stream_start(&params, err.as_mut_ptr(), err.len()) };
        if ptr.is_null() {
            return Err(cstr(&err));
        }
        Ok(Stream { ptr })
    }

    pub fn status(&self) -> (i32, bool, String, u64, i32, i32, u32, u32, i32) {
        let mut st: OlStatus = unsafe { std::mem::zeroed() };
        unsafe { ol_stream_status(self.ptr, &mut st) };
        (
            st.state,
            st.quit_error != 0,
            cstr(&st.message),
            st.frames_decoded,
            st.width,
            st.height,
            st.audio_rate,
            st.audio_channels,
            st.frames_lost,
        )
    }

    /// Neues Bild als RGBA, falls verfügbar.
    /// Anzahl der bisher empfangenen Ton-Samples (pro Kanal).
    pub fn audio_total(&self) -> u64 {
        let mut st: OlStatus = unsafe { std::mem::zeroed() };
        unsafe { ol_stream_status(self.ptr, &mut st) };
        st.audio_total
    }

    pub fn take_frame(&self, buf: &mut Vec<u8>) -> Option<(usize, usize)> {
        if buf.len() < 3840 * 2160 * 4 {
            buf.resize(3840 * 2160 * 4, 0);
        }
        let (mut w, mut h) = (0 as c_int, 0 as c_int);
        let got = unsafe { ol_stream_take_frame(self.ptr, buf.as_mut_ptr(), buf.len(), &mut w, &mut h) };
        if got != 0 {
            Some((w as usize, h as usize))
        } else {
            None
        }
    }

    pub fn take_audio(&self, buf: &mut [i16]) -> usize {
        unsafe { ol_stream_take_audio(self.ptr, buf.as_mut_ptr(), buf.len()) }
    }

    pub fn take_rumble(&self) -> Option<(u8, u8)> {
        let (mut l, mut r) = (0u8, 0u8);
        if unsafe { ol_stream_take_rumble(self.ptr, &mut l, &mut r) } != 0 {
            Some((l, r))
        } else {
            None
        }
    }

    pub fn set_input(&self, buttons: u32, l2: u8, r2: u8, lx: i16, ly: i16, rx: i16, ry: i16) {
        unsafe { ol_stream_set_input(self.ptr, buttons, l2, r2, lx, ly, rx, ry) }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        unsafe { ol_stream_stop(self.ptr) }
    }
}

#[allow(dead_code)]
pub fn _unused(_: &CStr) {}
